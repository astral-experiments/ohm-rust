//! Optional preoptimizer replay for a statically checked Rust caller-location record.
use rustc_codegen_ssa::CompiledModule;
use rustc_codegen_ssa::back::write::{CodegenContext, DwarfSourceFile};

use super::dwarf_cache::{Entry, copy_buffer, read_file};
use super::dwarf_typed_source as source;
use super::dwarf_typed_store::{Donor, Packet, Store, take};
use crate::{ModuleLlvm, llvm};

fn replay(
    module: &ModuleLlvm,
    donor: Donor<'_>,
    files: &[DwarfSourceFile],
    entry: &Entry,
) -> Option<CompiledModule> {
    let m = module.llmod();
    let mut context = [0u8; 32];
    if !unsafe { llvm::LLVMRustDwarfTypedContext(m, context.as_mut_ptr()) }
        || context != donor.context
    {
        return None;
    }
    let (mapping, proof) = source::mapping(donor.sources, files, entry.normalized_proofs())?;
    // The previous native is a regular work product whose complete hash was
    // checked by the existing line certificate. A two-miss refresh can leave a
    // still-valid, older packet behind. Refuse to pair it with today's `.o`.
    if entry.bound_donor()?.as_slice() != donor.object {
        return None;
    }
    let result = copy_buffer(unsafe {
        llvm::LLVMRustDwarfTypedMatch(
            m,
            donor.ordered.as_ptr(),
            donor.ordered.len(),
            donor.owners.as_ptr(),
            donor.owners.len(),
            mapping.as_ptr(),
            mapping.len(),
            proof.as_ptr(),
            proof.len(),
            donor.object.as_ptr(),
            donor.object.len(),
        )
    })?;
    entry.install_typed(&result)
}

fn snapshot(module: &ModuleLlvm, files: &[DwarfSourceFile]) -> Option<Packet> {
    let bytes = copy_buffer(unsafe { llvm::LLVMRustDwarfTypedSnapshot(module.llmod()) })?;
    let (context, mut rest) = bytes.split_at_checked(32)?;
    let ordered = take(&mut rest)?.to_vec();
    let owners = take(&mut rest)?.to_vec();
    let sources = source::capture(rest, files)?;
    Some(Packet { context: context.try_into().ok()?, ordered, owners, sources })
}

struct EmittedNames(std::ptr::NonNull<std::ffi::c_void>);

impl Drop for EmittedNames {
    fn drop(&mut self) {
        unsafe { llvm::LLVMRustDwarfDropEmittedNames(self.0.as_ptr()) }
    }
}

impl EmittedNames {
    fn start(module: &ModuleLlvm) -> Option<Self> {
        std::ptr::NonNull::new(unsafe { llvm::LLVMRustDwarfStartEmittedNames(module.llmod()) })
            .map(Self)
    }

    fn bind(&self, packet: &mut Packet) {
        if let Some(names) = copy_buffer(unsafe {
            llvm::LLVMRustDwarfFinishEmittedNames(
                self.0.as_ptr(),
                packet.owners.as_ptr(),
                packet.owners.len(),
            )
        }) {
            packet.owners = names;
        }
    }
}

enum Update {
    Enroll(Packet, Option<EmittedNames>),
    Defer,
}

pub(super) struct Attempt {
    store: Store,
    update: Update,
}

impl Attempt {
    /// Called after the ordinary normalized proof declined, while the original
    /// LLVM module and its target machine are both still alive.
    pub(super) fn prepare(
        cgcx: &CodegenContext,
        module: &ModuleLlvm,
        entry: &Entry,
    ) -> (Option<CompiledModule>, Option<Self>) {
        let Some((session, configuration, name, cold)) = entry.typed_session() else {
            return (None, None);
        };
        if cgcx.dwarf_typed_sources.is_empty() {
            return (None, None);
        }
        let mut shape = 0;
        if !unsafe { llvm::LLVMRustDwarfTypedShape(module.llmod(), module.tm.raw(), &mut shape) } {
            return (None, None);
        }
        let store = Store::new(session, configuration, name, shape);
        if let Some(bytes) = store.read()
            && let Some(donor) = Store::donor(&bytes)
            && let Some(compiled) = replay(module, donor, &cgcx.dwarf_typed_sources, entry)
        {
            return (Some(compiled), None);
        }
        let update = if store.should_enroll(cold) {
            let Some(packet) = snapshot(module, &cgcx.dwarf_typed_sources) else {
                return (None, None);
            };
            Update::Enroll(packet, None)
        } else {
            Update::Defer
        };
        (None, Some(Self { store, update }))
    }

    /// Bind native names only to the module actually selected for emission.
    pub(super) fn observe_emission(&mut self, module: &ModuleLlvm) {
        if let Update::Enroll(_, capture) = &mut self.update {
            *capture = EmittedNames::start(module);
        }
    }

    /// Publish only after rustc has successfully emitted the same module with no
    /// intervening diagnostics. Every filesystem error leaves native output valid.
    pub(super) fn finish(self, module: &CompiledModule) {
        if module.dwarf_object.is_some() || module.global_asm_object.is_some() {
            return;
        }
        let Some(path) = &module.object else { return };
        match self.update {
            Update::Enroll(mut packet, names) => {
                if let Some(object) = read_file(path) {
                    if let Some(names) = names {
                        names.bind(&mut packet);
                    }
                    self.store.publish(&packet, &object);
                }
            }
            Update::Defer => self.store.remember_miss(),
        }
    }
}
