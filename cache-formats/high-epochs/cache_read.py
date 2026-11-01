"""Offline Linux incremental-cache evidence checker. Run with Python 3.8+."""
import hashlib
import json
import re
import sys
import zipfile
from pathlib import Path

VERSIONS = {"unforced": "1.99.0-dev", "forced": "rustc-perf"}
FORMATS = {"base8":8, "old9":9, "old10":10, "new11":11, "new12":12, "integrated10":10}
NAMES = ["Null","Red","SideEffect","AnonZeroDeps","TraitSelect","CompileCodegenUnit","CompileMonoItem","Metadata"]
LOADED = "we asserted that the incremental cache should not be loaded"
REJECTED = "we asserted that an existing incremental cache directory should be successfully loaded"
TARGET = "x86_64-unknown-linux-gnu"
D64 = re.compile(r"[0-9a-f]{64}\Z")
LLVM = "lib/libLLVM.so.23.1-rust-1.99.0-beta"
LLHASH = "43bac9749e539cd5988a4d5452b64582cf07ac9e7c059fde29c7bee76e106c51"
FIXTURE = "68ee764b7d38e2c8add4a6961b422e998f3aa47611501a247b0c2da4733bc0f8"
def sha(data): return hashlib.sha256(data).hexdigest()
def gitblob(data): return hashlib.sha1(b"blob "+str(len(data)).encode()+b"\0"+data).hexdigest()
def check(value, reason):
    if not value: raise ValueError(reason)
def header(number, label):
    raw = label.encode()
    return b"RSIC"+number.to_bytes(2,"little")+bytes([len(raw)])+raw
def admits(buf, num, label):
    return (len(buf)>=7 and buf[:4]==b"RSIC" and int.from_bytes(buf[4:6],"little")==num
            and len(buf)>=7+buf[6] and buf[7:7+buf[6]]==label.encode())
def source(data, get):
    src=get("source/collision.json")
    rosters={}
    for label,row in src["query_blobs"].items():
        raw=data["source/"+label+"-queries.rs"]
        check(gitblob(raw)==row,"query source Git identity "+label)
        rosters[label]=NAMES+[x.decode() for x in re.findall(rb"^\s*query\s+(\w+)\(",raw,re.M)]
    for name,row in src["heads"].items():
        patch=data["source/review-"+name+".patch"]
        check(sha(patch)==row["patch_sha256"] and [line.split(" b/",1)[1] for line in patch.decode().splitlines() if line.startswith("diff --git ")]==row["production_and_test_paths"],"complete review patch "+name)
        check(row["parent"]==(src["named_base"]if name=="T1"else src["heads"]["T1"]["new_commit"]),"review stack")
    checks={}
    for role,old,new,peer,expected in [("T1",9,11,"base",1133),("T3",10,12,"integrated",1140)]:
        for stage,number in [("before",old),("after",new)]:
            raw=data["source/"+role+"-"+stage+".rs"]
            check(gitblob(raw)==src["header_blobs"][role][stage],"owning header blob")
            check(("const HEADER_FORMAT_VERSION: u16 = %d;"%number).encode() in raw,"actual numeric header")
            check(b'if buffer != rustc_version(sess).as_bytes()' in raw and b'if header_format_version != HEADER_FORMAT_VERSION' in raw,"reader conditions")
        a=data["source/"+role+"-before.rs"];b=data["source/"+role+"-after.rs"]
        decl=b"const HEADER_FORMAT_VERSION: u16 = "
        check(a.replace(decl+str(old).encode()+b";",decl+str(new).encode()+b";")==b,"one literal repair across the reviewed sources")
        hist=get("source/"+role+"-original-seed.json")["graph_census"]
        c=hist["by_number"]; lhs=rosters[role];rhs=rosters[peer]
        count=sum(n for i,n in enumerate(c) if i>=len(rhs) or lhs[i]!=rhs[i])
        check(len(c)==len(lhs) and sum(c)==hist["nodes"] and count==expected,"genuine earlier graph census")
        checks[role]={"changed_meaning_in_original_graph":count,"original_graph_nodes":hist["nodes"]}
        for version in VERSIONS.values():
            check(admits(header(old,version),old,version) and not admits(header(new,version),old,version)
                  and not admits(header(old,version),new,version) and admits(header(new,version),new,version),"before/after header")
    check([len(rosters[k])for k in ("base","T1","T3","integrated")]==[361,363,364,362],"number of actual query kinds")
    check(rosters["T3"][190]=="trait_impl_receiver_adt" and rosters["integrated"][190]=="dyn_compatibility_violations","real old tag alias")
    check(src["pre_repair_exit"]==1 and src["post_repair_exit"]==0,"preserved independent header acceptance exits")
    for when,want in [("current",True),("repaired",False)]:
        observation=get("source/"+when+".json")
        check(observation["same_header_isolation_pass"]== (not want) and observation["native_runs"]==0 and len(observation["comparisons"])==4,"separate source-only replay")
        check(all(v=={"candidate_reads_foreign_prefix":want,"candidate_reads_own_prefix":True,"foreign_reads_candidate_prefix":want}for v in observation["comparisons"].values()),"both directions")
    return checks
def behavior(row, want):
    check(row["compile"]["exit"]==0 and row["compile"]["stderr"]==row["compile"]["stdout"]=="","successful compilation")
    check(row["run"]=={"argv":["/tmp/w/probe"],"exit":0,"stderr":"","stdout":want+"\n"},"actual program value")
def diagnostics(row):
    check(row["compile"]["exit"]==1 and not row.get("run"),"expected compile failure")
    errors=[]
    for line in row["compile"]["stderr"].splitlines():
        j=json.loads(line)
        if j.get("level")=="error" and (j.get("spans") or j.get("code")):
            errors.append({n:j.get(n) for n in ("code","message","level","spans","children")})
    check(row["diagnostics"]==errors and any(d.get("code",{}).get("code")=="E0277" and "Left" in d["message"] and
          any(s["file_name"]=="probe.rs" and s["is_primary"] for s in d["spans"])for d in errors if d.get("code")),"real native E0277 and source span")
    return errors
def assertion(rows, own):
    good,bad=(("assert_loaded","assert_not_loaded")if own else("assert_not_loaded","assert_loaded"))
    behavior(rows[good],"17");x=rows[bad]
    check(x["compile"]["exit"]==1 and not x.get("run") and (LOADED if own else REJECTED) in x["compile"]["stderr"]
          and (REJECTED if own else LOADED) not in x["compile"]["stderr"],"complementary native cache-state assertions")
    return "loaded" if own else "not-loaded"
def images(get,manifest,scope):
    subjects={}
    for tag,packed in manifest["sdk_packets"].items():
        a=get("images/"+tag+".json");inv=get("images/"+tag+"-manifest.json")
        check(packed==a["raw_packet_sha256"] and D64.fullmatch(packed) and len(inv)==73,"whole qualified SDK receipt")
        check(a["header_format"]==FORMATS[tag] and inv[a["driver_relative"]][0]==a["driver_sha256"] and
              inv["bin/rustc"][0]==a["launcher_sha256"] and inv[LLVM][:2]==[LLHASH,208144480],"actual launcher, driver and LLVM")
        check(a["kernel_fixture_zip_sha256"] and D64.fullmatch(a["kernel_fixture_zip_sha256"])
              and a["native_build_exit"]==a["suite_exit"]==0 and a["suite_pass_line"].endswith(" ... ok"),"official fixture outcome")
        out=a["stdout_by_solver"]; check(set(out)=={"default","next"} and out["default"]==out["next"],"both native solvers")
        for solver,series in out.items():
            seen=sorted((z for z in a["readers"] if z["solver"]==solver),key=lambda z:z["stage"])
            check(len(seen)==len(series) and [z["stage"] for z in seen]==list(range(len(series))) and bool(seen),"all real revisions")
            for z,want in zip(seen,series):
                ct=z["compiler"];rt=z["program"]
                check(any('execve("$SDK/bin/rustc"'in x and '"reader.rs"'in x and ' = 0' in x for x in ct),"actual native compiler exec")
                check(any('PROT_READ|PROT_EXEC' in x and '$SDK/'+a["driver_relative"]+'>'in x and ' = 0x'in x for x in ct),"selected mapped driver")
                check(any('PROT_READ|PROT_EXEC' in x and '$SDK/'+LLVM+'>'in x and ' = 0x'in x for x in ct),"selected mapped LLVM")
                check(any("exit_group(0)"in x for x in ct) and any("exit_group(0)"in x for x in rt),"reader and program exited")
                check(any('execve("$WORK/reader"'in x for x in rt) and any('write(1<' in x and json.dumps(want) in x for x in rt),"executed program's actual stdout")
        if tag in ("new11","new12"):
            check(a["commit"]=={"new11":"97ad3866dfe788d2ed0bd8f945a9972423da2423","new12":"26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9"}[tag],"exact repaired source")
        if tag=="integrated10":
            check(a["commit"]=="c5178286c2661c22ef4ed304d7213589483548ee" and a["tree"]=="bbfe1faca29784b74e01830873eae7d8f27b52f0"
                  and a["driver_sha256"]=="5496f30e68382082dfee8a190963b757231f005f41ad6bbfb4ecf5dbd7393f11"
                  and "rdr-public-hash-native-exports ... ok" in a["integrated_suite_line"],"integrated source's own run-make")
        subjects[tag]=a
    pins={"base8":("625207d26ce4449c9c95d871c88cfb50e904c09f","125fdd522d1984d479cd4bbbe634df787906efd2934c43a5e443ff8d45498d4f"),
          "old9":("6d997cb8c8ebd3cb0e16c28c09d9855d862238a2","c95f12f07e7a052796f9f35e2331b393efa194bdf348041e165d08b70bcd8023"),
          "old10":("3334fcb9306dadf768e58248c4fefb53852fe02d","9793bfacc271ebd0880ba5e05a82d56ac025f86f1b48b3547beb31471230dee7"),
          "new11":("97ad3866dfe788d2ed0bd8f945a9972423da2423","d043561dbbec02881096b8f459673f1e509d2fb1fccce733c7c95b13073f1537"),
          "new12":("26f574759c0d44bebd2d11c0bbe0e2ba0042d1f9","45e1a53eea14bc393881e3d9a1cc1ce87fc750d6f8cabf20b10c77a14f308824")}
    for k in subjects.keys() & pins.keys():check((subjects[k]["commit"],subjects[k]["driver_sha256"])==pins[k],"registered source and real driver")
    check(set(subjects)==(set(FORMATS)-{"integrated10"} if scope=="mandatory" else {"new11","new12","integrated10"}),"required source arms")
    check(len({a["driver_sha256"] for a in subjects.values()})==len(subjects),"distinct compiler drivers")
    return subjects
def earlier(get):
    evidence=get("source/earlier-unforced.json"); states={}
    encoded=lambda v:(json.dumps(v,sort_keys=True,indent=2)+"\n").encode()
    check(evidence["raw_closed_sha256"]=="94e1e5e7d55bb1c41d159b87efc38713d075d4ccaffa2404cc510f859e853814","original 8/9/10 close")
    subjects=("base","T1","T3")
    for b in subjects:
        clean=evidence["clean"][b]
        check(sha(encoded(clean))==evidence["raw_inputs"][b+"-clean.json"],"original clean record")
        behavior(clean["B"],"17");diagnostics(clean["E"])
        for a in subjects:
            k=a+"-to-"+b;rows=evidence["pairs"][k]
            check(sha(encoded(rows))==evidence["raw_inputs"][k+".json"],"original legacy transition")
            behavior(rows["no_edit"],"11");behavior(rows["positive"],"17")
            check(diagnostics(rows["negative"])==diagnostics(clean["E"]),"previous clean diagnostics")
            states[k]=assertion(rows,a==b)
    check(states==evidence["states"] and len(states)==9,"previous original unforced transitions")
    return states
def validate_row(row, edit, gate, reader, seed, fresh, force, hashes):
    check(row["worker_exit"]==0 and row["forced_rustc_version"]==row["perf_artifact_env"]==force,"successful worker and actual environment")
    check((row["edit"],row["assertion"],row["source_sha256"])==(edit,gate,hashes[edit]),"literal fixture and asserted mode")
    argv=["/tmp/sdk/bin/rustc","probe.rs","--crate-name","ohm_cache_migration","--edition","2024","--sysroot","/tmp/sdk","--emit=link","-Ccodegen-units=1","-Cmetadata=ohm_cache_test","-Cincremental=/tmp/w/incremental","--error-format=json","-o","/tmp/w/probe"]
    if gate!="ordinary":argv.append("-Zassert-incr-state="+gate)
    check(row["compile"]["argv"]==argv,"same-path actual native compiler invocation")
    for k in ("selected_driver_identity","selected_llvm_identity","driver_path","llvm_path"):
        check(row[k]==reader[k],"actual native image vs writer seed")
    check(row["prior_headers"]==({}if fresh else seed["seed_headers"]) and row["seed_sha256"]==("-"if fresh else seed["seed_gz_sha256"]),"original restored prior graph")
def matrix(get, scope, mode, sdk, hashes):
    pre=mode+"/";seeds={};empty={};force="rustc-perf"if mode=="forced"else None
    accepted=get(pre+"receipt.json");formats={r:FORMATS[r]for r in sdk}
    for tag,arm in sdk.items():
        s=seeds[tag]=get(pre+tag+"-seed.json");row=empty[tag]=get(pre+tag+"-empty.json")
        validate_row(s,"A","ordinary",s,{},True,force,hashes);behavior(s,"11")
        check(s["launcher_sha256"]==arm["launcher_sha256"] and s["driver_sha256"]==arm["driver_sha256"] and s["llvm_sha256"]==LLHASH,"selected seed bytes")
        check(s["driver_path"]=="/tmp/sdk/"+arm["driver_relative"] and s["llvm_path"]=="/tmp/sdk/"+LLVM,"loader image selection")
        check(s["rustc"]["exit"]==0 and "host: "+TARGET in s["rustc"]["stdout"].splitlines(),"actual tool version")
        check(s["selected_llvm_identity"][2]==208144480 and s["selected_driver_identity"][2]==arm["driver_bytes"],"actual loader file sizes")
        check(set(Path(p).name for p in s["seed_headers"])=={"dep-graph.bin","query-cache.bin","work-products.bin"} and
              all(h["version"]==VERSIONS[mode] and h["format"]==formats[tag] and h["bytes"]>len(header(formats[tag],VERSIONS[mode])) and D64.fullmatch(h["sha256"])for h in s["seed_headers"].values()),"all stored native header-bearing files")
        for name,e,assertion_name in [("B","B","ordinary"),("E","E","ordinary"),("empty_loaded","B","loaded"),("empty_not_loaded","B","not-loaded")]:
            validate_row(row[name],e,assertion_name,s,{},True,force,hashes)
        behavior(row["B"],"17");diagnostics(row["E"]);assertion({"assert_loaded":row["empty_loaded"],"assert_not_loaded":row["empty_not_loaded"]},False)
    states={}
    for a in sdk:
        for b in sdk:
            if not(a==b or ((a.startswith("new") or b.startswith("new")) if scope=="mandatory" else "integrated10" in (a,b))):continue
            k=a+"-to-"+b;rows=get(pre+k+".json")
            labels=[("no_edit","A","ordinary"),("positive","B","ordinary"),("negative","E","ordinary"),("assert_loaded","B","loaded"),("assert_not_loaded","B","not-loaded")]
            check(set(rows)=={k for k,_,_ in labels},"full five observations per pair")
            for name,edit,gate in labels:validate_row(rows[name],edit,gate,seeds[b],seeds[a],False,force,hashes)
            behavior(rows["no_edit"],"11");behavior(rows["positive"],"17")
            check(diagnostics(rows["negative"])==diagnostics(empty[b]["E"]),"complete native clean and incremental diagnostics")
            states[k]=assertion(rows,a==b)
    check(accepted["states"]==states and accepted["mode"]==mode and accepted["native_performance_rows"]==0,"registered final cohort")
    check(accepted["own_cache_loaded"]==sum(s=="loaded" for s in states.values()) and
          accepted["different_format_rejected"]==sum(s=="not-loaded"for s in states.values()) and accepted["genuine_empty_controls"]==len(sdk),"complete controls")
    return states
def verify(path):
    with zipfile.ZipFile(path)as z:
        check(len(z.infolist())<=120 and sum(f.file_size for f in z.infolist())<12<<20 and len(z.namelist())==len(set(z.namelist())),"bounded unique evidence members")
        data={n:z.read(n)for n in z.namelist()}
    get=lambda name:json.loads(data[name])
    m=get("manifest.json");check({n:sha(v)for n,v in data.items()if n!="manifest.json"}==m["members"],"exact member hashes")
    scope=m["scope"];check(scope in ("mandatory","integrated"),"matrix type")
    fixture=data["probe.rs"];check(sha(fixture)==FIXTURE and fixture.count(b"        11\n")==fixture.count(b"impl Score for Left {")==1,"original probe")
    hashes={"A":sha(fixture),"B":sha(fixture.replace(b"        11\n",b"        17\n")),"E":sha(fixture.replace(b"impl Score for Left {",b"impl Score for Right {"))}
    check(set(m["raw_native_inputs"])==set(VERSIONS),"both independent configuration modes")
    for mode,files in m["raw_native_inputs"].items():
        for name,h in files.items():check(sha(data[mode+"/"+name])==h,"original raw native input "+name)
    collision=source(data,get);old_states=earlier(get);sdk=images(get,m,scope)
    states={v:matrix(get,scope,v,sdk,hashes)for v in VERSIONS}
    for mode,rows in states.items():
        want={r+s for r in sdk for s in ("-seed.json","-empty.json")}|{k+".json" for k in rows}
        check(set(m["raw_native_inputs"][mode])==want and m["modes"][mode]["four_acquired_leases"]==4, "every raw seed and directional result")
        check(all(D64.fullmatch(m["modes"][mode][k])for k in ("original_inputs_sha256","original_accepted_sha256")),"original native closure hashes")
        reg=m["registrations"]["mandatory" if scope=="mandatory" else "integrated"]
        check(get(mode+"/receipt.json")["registration"]==reg,"native registration on each mode")
    if scope=="mandatory":
        recovery=get("source/manifest-lookup.json")
        check(recovery["native_workers"]==0 and recovery["failure"]=="KeyError: 'sdk_manifest'" and
              recovery["registration_sha256"]==m["registrations"]["corrected_legacy_manifest"]=="7574eac59c8a625fc965e59fc6f1d2946c0873efac25e5c458455c569f189db0","preserved failed intake")
        for before,tag in (("base","base8"),("T1","old9"),("T3","old10")):
            check(sdk[tag]["sdk_manifest_sha256"]==recovery["original_sdk_manifests"][before] and
                  sha(data["images/"+tag+"-manifest.json"])==sdk[tag]["sdk_manifest_sha256"],"genuine legacy manifest in native ZIP")

    return {"scope":scope,"drivers":{k:v["driver_sha256"]for k,v in sdk.items()},"source_regression":collision,
            "states":states,"previous_unforced_8_9_10":old_states,"own_per_mode":len(sdk),"cross_per_mode":len(states["unforced"])-len(sdk),"native_performance_rows":0}
if __name__=="__main__":
    if sys.flags.optimize or len(sys.argv)<2:raise SystemExit("python3 -ISB verify.py mandatory.zip [integrated-v10.zip]")
    results=[verify(p)for p in sys.argv[1:]]
    if len(results)==2:
        check({a["scope"]for a in results}=={"mandatory","integrated"} and all(results[0]["drivers"][r]==results[1]["drivers"][r]for r in ("new11","new12")),"same two repaired SDKs across packets")
    print(json.dumps(results,sort_keys=True,indent=2))
