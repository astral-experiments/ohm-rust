import hashlib,json,pathlib,tarfile,urllib.request,io,gzip
root=pathlib.Path('/tmp/ohm-target-size-prototypes')
manifest=json.loads((root/'rustc-perf-fixtures.json').read_text())
revision=manifest['revision']
archive=root/'inputs/rustc-perf-source.tar.gz'
if not archive.exists():
 partial=archive.with_suffix('.partial')
 urllib.request.urlretrieve(
  f'https://codeload.github.com/rust-lang/rustc-perf/tar.gz/{revision}',partial)
 assert hashlib.sha256(partial.read_bytes()).hexdigest()==manifest['source_archive_sha256']
 partial.rename(archive)
assert hashlib.sha256(archive.read_bytes()).hexdigest()==manifest['source_archive_sha256']
selected=root/'inputs/rustc-perf-fixtures.tar.gz'
with tarfile.open(archive,'r:gz') as incoming, selected.open('wb') as raw:
 with gzip.GzipFile(fileobj=raw,mode='wb',mtime=0,filename='') as compressed:
  with tarfile.open(fileobj=compressed,mode='w') as outgoing:
   for entry in incoming:
    parts=pathlib.PurePosixPath(entry.name).parts
    if (len(parts)>3 and parts[1:3]==('collector','compile-benchmarks')
        and parts[3] in manifest['fixtures']):
     entry.name='/'.join(parts[3:])
     outgoing.addfile(entry,incoming.extractfile(entry) if entry.isfile() else None)
with tarfile.open(selected) as stream:
 members={m.name:hashlib.sha256(stream.extractfile(m).read()).hexdigest()
          for m in stream if m.isfile()}
actual=hashlib.sha256(json.dumps(members,sort_keys=True,separators=(',',':')).encode()).hexdigest()
assert actual==manifest['contents_sha256'],actual
manifest['archive_sha256']=hashlib.sha256(selected.read_bytes()).hexdigest()
manifest['archive_bytes']=selected.stat().st_size
(root/'rustc-perf-fixtures.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest))
