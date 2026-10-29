#![feature(rustc_private)]
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

use std::borrow::Cow;
use rustc_data_structures::fx::{FxHashMap, FxHashSet};
use rustc_driver::Callbacks;
use rustc_interface::interface::Config;
use rustc_session::config::{CheckCfg, ExpectedValues};
use rustc_span::{Symbol, sym};
use rustc_target::spec::Target;

const NAMES: [Symbol; 9] = [
    sym::target_abi, sym::target_arch, sym::target_endian, sym::target_env,
    sym::target_family, sym::target_object_format, sym::target_os,
    sym::target_pointer_width, sym::target_vendor,
];
type Map = FxHashMap<Symbol, ExpectedValues<Symbol>>;

fn prefill(count: usize, family_any: bool) -> Map {
    let mut result = Map::default();
    for (index, name) in NAMES.into_iter().enumerate() {
        let mut values = FxHashSet::default();
        for i in 0..count { values.insert(Some(Symbol::intern(&format!("user-{i}")))); }
        if index != 4 && (index + count) % 2 == 1 { values.insert(None); }
        result.insert(name, if index == 4 && family_any {
            ExpectedValues::Any
        } else {
            ExpectedValues::Some(values)
        });
    }
    result
}

fn one(map: &mut Map, name: Symbol, value: Symbol) {
    if let ExpectedValues::Some(values) = map.get_mut(&name).unwrap() {
        values.insert(Some(value));
    }
}

fn older(map: &mut Map, current: &Target) {
    for t in Target::builtins().chain(std::iter::once(current.clone())) {
        one(map, sym::target_abi, t.options.cfg_abi.desc_symbol());
        one(map, sym::target_arch, t.arch.desc_symbol());
        one(map, sym::target_endian, t.options.endian.desc_symbol());
        one(map, sym::target_env, t.options.env.desc_symbol());
        if let ExpectedValues::Some(families) = map.get_mut(&sym::target_family).unwrap() {
            families.extend(t.options.families.iter().map(|s| Some(Symbol::intern(s))));
        }
        one(map, sym::target_object_format, t.options.binary_format.desc_symbol());
        one(map, sym::target_os, t.options.os.desc_symbol());
        one(map, sym::target_pointer_width, sym::integer(t.pointer_width));
        one(map, sym::target_vendor, t.vendor_symbol());
    }
}

fn read(map: &Map) -> Vec<Option<(usize, Vec<Option<u32>>)>> {
    NAMES.iter().map(|name| match &map[name] {
        ExpectedValues::Any => None,
        ExpectedValues::Some(items) => Some((
            items.capacity(), items.iter().map(|v| v.map(|s| s.as_u32())).collect()
        )),
    }).collect()
}

struct Reader;
impl Callbacks for Reader {
    fn config(&mut self, cfg: &mut Config) {
        cfg.track_state = Some(Box::new(|session| {
            let actual: &Map = &session.check_config.expecteds;
            let haiku = Symbol::intern("haiku");
            let (n, os_count, present) = std::thread::scope(|s| s.spawn(|| {
                let ExpectedValues::Some(values) = &actual[&sym::target_os] else { panic!() };
                (actual.len(), values.len(), values.contains(&Some(haiku)))
            }).join().unwrap());
            assert!(n >= 20 && os_count == 50 && present);
            eprintln!("complete-cfg rows={n} target_os={os_count} inactive={present}");

            let mut cases = 0;
            for shape in ["native", "bare", "many", "family-any"] {
                for count in [0, 4, 11, 27] {
                    let name = if shape == "native" { "x86_64-unknown-linux-gnu" } else { "thumbv7em-none-eabi" };
                    let mut target = Target::builtins().find(|t| t.llvm_target == name).unwrap();
                    if shape == "many" || shape == "family-any" {
                        let private = format!("private-callback-{shape}-{count}");
                        let mut families = vec![Cow::Owned(private); 17];
                        families.extend([Cow::Borrowed("unix"), Cow::Borrowed("wasm")]);
                        target.options.families = families.into();
                    }
                    let is_any = shape == "family-any";
                    let mut oracle = prefill(count, is_any);
                    let mut observed = CheckCfg {
                        exhaustive_names: true,
                        exhaustive_values: true,
                        expecteds: prefill(count, is_any),
                        ..Default::default()
                    };
                    older(&mut oracle, &target);
                    let before = Symbol::intern(&format!("before-callback-{shape}-{count}")).as_u32();
                    observed.fill_well_known(&target);
                    let after = Symbol::intern(&format!("after-callback-{shape}-{count}")).as_u32();
                    assert_eq!(after, before + 1, "{shape}-{count} interner");
                    std::thread::scope(|s| s.spawn(|| {
                        assert_eq!(read(&oracle), read(&observed.expecteds), "{shape}-{count}");
                    }).join().unwrap());
                    if shape == "bare" && count == 4 {
                        let ExpectedValues::Some(items) = &observed.expecteds[&sym::target_family] else { panic!() };
                        assert_eq!((items.len(), items.capacity()), (7, 14));
                    }
                    cases += 1;
                }
            }
            eprintln!("exact-cfg-callback cases={cases} tested_buckets={} foreign_thread=true lazy_any=true", cases * NAMES.len());
        }));
    }
}
fn main() {
    rustc_driver::run_compiler(&std::env::args().collect::<Vec<_>>(), &mut Reader);
}
