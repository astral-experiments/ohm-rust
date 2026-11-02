#include "llvm-c/Core.h"
#include "llvm-c/TargetMachine.h"

#ifdef LLVM_RUSTC_OHM_COALESCER
#include "llvm/IR/LegacyPassManager.h"
#include "llvm/Target/TargetMachine.h"

// These helpers have external linkage in the original LLVM translation unit.
#define CoalescerPair RustCoalescerPair
#include "RegisterCoalescer.cpp"
#undef CoalescerPair

namespace {
// The original pass registration supplies analysis identity and diagnostic names.
// Intercept only the built-in pass actually selected by the target.
class RustCoalescerManager final : public legacy::PassManager {
public:
  void add(Pass *P) override {
    if (P->getPassID() == &llvm::RegisterCoalescerID) {
      delete P;
      P = new RegisterCoalescerLegacy();
    }
    legacy::PassManager::add(P);
  }
};

bool canUseRustCoalescer(TargetMachine &TM, bool Allow) {
  if (!Allow || TM.getOptLevel() != CodeGenOptLevel::Less ||
      TM.getTargetTriple().str() != "x86_64-unknown-linux-gnu")
    return false;
  const auto &Options = cl::getRegisteredOptions();
  if (auto *Stats = Options.lookup("stats"); Stats && Stats->getNumOccurrences())
    return false;
  for (const char *Name : {"join-liveintervals", "terminal-rule", "join-splitedges",
                           "join-globalcopies", "verify-coalescing",
                           "late-remat-update-threshold", "large-interval-size-threshold",
                           "large-interval-freq-threshold"}) {
    auto *Option = Options.lookup(Name);
    if (!Option || Option->getNumOccurrences())
      return false;
  }
  return true;
}
} // namespace

extern "C" LLVMPassManagerRef LLVMRustCreateCoalescingPassManager(
    LLVMTargetMachineRef Target, bool Allow) {
  TargetMachine &TM = *reinterpret_cast<TargetMachine *>(Target);
  if (canUseRustCoalescer(TM, Allow))
    return wrap(new RustCoalescerManager());
  return LLVMCreatePassManager();
}
#else
extern "C" LLVMPassManagerRef LLVMRustCreateCoalescingPassManager(
    LLVMTargetMachineRef, bool) {
  return LLVMCreatePassManager();
}
#endif
