#include "llvm-c/Core.h"
#include "llvm-c/TargetMachine.h"

#ifdef LLVM_RUSTC_OHM_COALESCER
#include "llvm/CodeGen/LiveDebugValuesPass.h"
#include "llvm/CodeGen/TargetPassConfig.h"
#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/LegacyPassManager.h"
#include "llvm/IR/Module.h"
#include "llvm/Target/TargetMachine.h"

// These helpers have external linkage in the original LLVM translation unit.
#define CoalescerPair RustCoalescerPair
#include "RegisterCoalescer.cpp"
#undef CoalescerPair

namespace {
// Inlined variables may survive even when the root compile unit only has line
// tables. Inspect the late machine state before dropping range propagation.
bool hasNoVariableLocations(const MachineFunction &MF) {
  const DISubprogram *SP = MF.getFunction().getSubprogram();
  if (!SP || !SP->getUnit() ||
      SP->getUnit()->getEmissionKind() != DICompileUnit::LineTablesOnly ||
      !SP->getRetainedNodes().empty() || !MF.getVariableDbgInfo().empty() ||
      !MF.DebugValueSubstitutions.empty() || !MF.DebugPHIPositions.empty())
    return false;

  for (const MachineBasicBlock &MBB : MF)
    for (const MachineInstr &MI : MBB.instrs())
      if (MI.isDebugInstr() || MI.peekDebugInstrNum())
        return false;
  return true;
}

class LineTableDebugValues final : public MachineFunctionPass {
  // The pinned new-PM entry shares the legacy implementation and requests no
  // analyses from this manager.
  MachineFunctionAnalysisManager Analyses;

public:
  LineTableDebugValues() : MachineFunctionPass(llvm::LiveDebugValuesID) {}

  void getAnalysisUsage(AnalysisUsage &AU) const override {
    AU.setPreservesCFG();
    AU.addRequired<TargetPassConfig>();
    MachineFunctionPass::getAnalysisUsage(AU);
  }

  bool runOnMachineFunction(MachineFunction &MF) override {
    if (hasNoVariableLocations(MF))
      return false;
    bool EntryValues = getAnalysis<TargetPassConfig>()
                           .getTM<TargetMachine>()
                           .Options.ShouldEmitDebugEntryValues();
    return !LiveDebugValuesPass(EntryValues).run(MF, Analyses).areAllPreserved();
  }
};

// The original pass registration supplies analysis identity and diagnostic names.
// Intercept only the built-in pass actually selected by the target.
class RustOutputPassManager final : public legacy::PassManager {
  bool HasLineTables;

public:
  explicit RustOutputPassManager(const Module &M)
      : HasLineTables(llvm::any_of(M.debug_compile_units(), [](const DICompileUnit *CU) {
          return CU->getEmissionKind() == DICompileUnit::LineTablesOnly;
        })) {}

  void add(Pass *P) override {
    if (P->getPassID() == &llvm::RegisterCoalescerID) {
      delete P;
      P = new RegisterCoalescerLegacy();
    } else if (HasLineTables && P->getPassID() == &llvm::LiveDebugValuesID) {
      delete P;
      P = new LineTableDebugValues();
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

extern "C" LLVMPassManagerRef LLVMRustCreateOutputPassManager(
    LLVMTargetMachineRef Target, LLVMModuleRef M, bool Allow) {
  TargetMachine &TM = *reinterpret_cast<TargetMachine *>(Target);
  if (canUseRustCoalescer(TM, Allow))
    return wrap(new RustOutputPassManager(*unwrap(M)));
  return LLVMCreatePassManager();
}
#else
extern "C" LLVMPassManagerRef LLVMRustCreateOutputPassManager(
    LLVMTargetMachineRef, LLVMModuleRef, bool) {
  return LLVMCreatePassManager();
}
#endif
