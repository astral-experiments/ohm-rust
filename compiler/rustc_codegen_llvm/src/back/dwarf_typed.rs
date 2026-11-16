//! Optional preoptimizer replay for a statically checked Rust caller-location record.
use rustc_codegen_ssa::CompiledModule;
use rustc_codegen_ssa::back::write::{CodegenContext, DwarfSourceFile};

use super::dwarf_cache::{Entry, read_file};
use super::dwarf_typed_store::{Donor, Packet, Store, take};
use super::{dwarf_caller, dwarf_typed_source as source};
use crate::{ModuleLlvm, llvm};

pub(super) fn buffer(value: Option<&'static mut llvm::Buffer>) -> Option<Vec<u8>> {
    let value = value?;
    let size = unsafe { llvm::LLVMRustBufferLen(value) };
    let bytes = (size <= 128 << 20).then(|| unsafe {
        std::slice::from_raw_parts(llvm::LLVMRustBufferPtr(value), size).to_vec()
    });
    unsafe { llvm::LLVMRustBufferFree(value) };
    bytes
}

fn replay(
    module: &ModuleLlvm,
    donor: Donor<'_>,
    files: &[DwarfSourceFile],
    entry: &Entry,
    store: &Store,
) -> Option<CompiledModule> {
    let m = module.llmod();
    let mut context = [0u8; 32];
    if !unsafe { llvm::LLVMRustDwarfTypedContext(m, context.as_mut_ptr()) }
        || context != donor.context
    {
        return None;
    }
    let mapping = source::mapping(donor.sources, files)?;
    // The previous native is a regular work product whose complete hash was
    // checked by the existing line certificate. A two-miss refresh can leave a
    // still-valid, older packet behind. Refuse to pair it with today's `.o`.
    if entry.bound_donor()?.as_slice() != donor.object {
        return None;
    }
    let mut next = None;
    let result = if dwarf_caller::is_projected(donor.ordered) {
        let current = entry.caller.as_deref()?;
        let changes = dwarf_caller::changes(donor.ordered, current, &mapping)?;
        unsafe {
            llvm::LLVMRustDwarfApplyCallerChanges(
                m,
                current.as_ptr(),
                current.len(),
                donor.owners.as_ptr(),
                donor.owners.len(),
                mapping.bytes.as_ptr(),
                mapping.bytes.len(),
                changes.as_ptr(),
                changes.len(),
                donor.object.as_ptr(),
                donor.object.len(),
                &mut next,
            )
        }
    } else {
        unsafe {
            llvm::LLVMRustDwarfTypedMatchAndSnapshot(
                m,
                donor.ordered.as_ptr(),
                donor.ordered.len(),
                donor.owners.as_ptr(),
                donor.owners.len(),
                mapping.bytes.as_ptr(),
                mapping.bytes.len(),
                mapping.witness.as_ptr(),
                mapping.witness.len(),
                donor.object.as_ptr(),
                donor.object.len(),
                &mut next,
            )
        }
    };
    let next = buffer(next);
    let result = buffer(result)?;
    let (compiled, stored) = entry.install_typed(&result)?;
    if stored && let Some(packet) = next.and_then(|bytes| packet(&bytes, files)) {
        // Advance the typed donor only after the ordinary object and its
        // certificate have been installed for this incremental session.
        store.publish(&packet, &result);
    }
    Some(compiled)
}

fn snapshot(
    module: &ModuleLlvm,
    files: &[DwarfSourceFile],
    caller: Option<&[u8]>,
) -> Option<Packet> {
    let bytes = buffer(unsafe {
        match caller {
            Some(proof) => {
                llvm::LLVMRustDwarfProjectedSnapshot(module.llmod(), proof.as_ptr(), proof.len())
            }
            None => llvm::LLVMRustDwarfTypedSnapshot(module.llmod()),
        }
    })?;
    packet(&bytes, files)
}

fn packet(bytes: &[u8], files: &[DwarfSourceFile]) -> Option<Packet> {
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
        if let Some(names) = buffer(unsafe {
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
            && let Some(compiled) = replay(module, donor, &cgcx.dwarf_typed_sources, entry, &store)
        {
            return (Some(compiled), None);
        }
        let update = if store.should_enroll(cold) {
            let Some(packet) = snapshot(module, &cgcx.dwarf_typed_sources, entry.caller.as_deref())
            else {
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
