#include "llvm-c/Types.h"
#include "llvm/Config/llvm-config.h"
#include <cstddef>
#include <cstdint>

using SourcePlacementCallback = void (*)(void *, const uint8_t *, size_t);

#if LLVM_VERSION_MAJOR == 23
#include "llvm/ADT/DenseMap.h"
#include "llvm/ADT/SmallPtrSet.h"
#include "llvm/ADT/SmallString.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/ADT/StringExtras.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/DebugProgramInstruction.h"
#include "llvm/IR/GlobalObject.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Metadata.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/TrackingMDRef.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/SHA256.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/TargetParser/Triple.h"
#include "llvm/Transforms/Utils/Cloning.h"
#include <algorithm>
#include <iterator>
#include <optional>


namespace llvm {
extern LLVM_ABI cl::opt<bool> PickMergedSourceLocations;
}

using namespace llvm;

namespace {
using FileName = std::pair<StringRef, StringRef>;

static void appendInteger(std::string &Out, uint64_t N) {
  do {
    uint8_t Byte = N & 0x7f;
    N >>= 7;
    Out.push_back(static_cast<char>(Byte | (N ? 0x80 : 0)));
  } while (N);
}

static void appendString(std::string &Out, StringRef S) {
  appendInteger(Out, S.size());
  Out.append(S.data(), S.size());
}

using NodeIds = DenseMap<MDNode *, size_t>;
using Signature = std::pair<unsigned, StringRef>;
using SourcePoint = std::pair<uint64_t, unsigned>;

struct Placements {
  struct InputFile { FileName Name; Signature Check; size_t Node; };
  struct File { FileName Name; Signature Check; size_t Begin, End, FirstOrdinary; };
  const NodeIds &IDs;
  SmallVector<InputFile, 0> Inputs;
  SmallVector<File, 0> Files;
  SmallVector<uint32_t, 0> FileOfNode;
  // The high word first identifies an LLVM metadata node, then its sorted file.
  // The low word is the physical source line. Each file gets one contiguous run.
  SmallVector<uint64_t, 0> Points;
  SmallVector<unsigned, 0> Absolute{0, 1};

  explicit Placements(const NodeIds &IDs) : IDs(IDs) {}

  bool recordFile(DIFile *F) {
    Signature S{0, StringRef()};
    if (auto C = F->getChecksum()) {
      unsigned Width = C->Kind == DIFile::CSK_MD5 ? 32 : C->Kind == DIFile::CSK_SHA1 ? 40 : C->Kind == DIFile::CSK_SHA256 ? 64 : 0;
      if (!Width || C->Value.size() != Width || !llvm::all_of(C->Value, [](char B) { return isHexDigit(B); })) return false;
      S = {static_cast<unsigned>(C->Kind), C->Value};
    }
    auto I = IDs.find(F);
    if (I == IDs.end()) return false;
    Inputs.push_back({{F->getDirectory(), F->getFilename()}, S, I->second});
    return true;
  }

  bool recordLine(DIFile *F, unsigned Line) {
    auto I = IDs.find(F);
    if (I == IDs.end() || I->second >= UINT32_MAX) return false;
    Points.push_back((uint64_t(I->second) << 32) | Line);
    return true;
  }

  bool finish() {
    if (IDs.size() >= UINT32_MAX || Points.size() >= UINT32_MAX-2) return false;
    FileOfNode.assign(IDs.size(), UINT32_MAX);
    llvm::sort(Inputs, [](const auto &A, const auto &B) { return A.Name < B.Name; });
    for (const auto &I : Inputs) {
      if (Files.empty() || Files.back().Name != I.Name)
        Files.push_back({I.Name, I.Check, 0, 0, 0});
      else if (Files.back().Check != I.Check)
        return false;
      FileOfNode[I.Node] = Files.size() - 1;
    }
    for (auto &P : Points) {
      unsigned File = FileOfNode[P >> 32], Line = static_cast<uint32_t>(P);
      if (File == UINT32_MAX) return false;
      P = (uint64_t(File) << 32) | Line;
      Absolute.push_back(Line);
    }
    llvm::sort(Points); Points.erase(std::unique(Points.begin(), Points.end()), Points.end());
    llvm::sort(Absolute); Absolute.erase(std::unique(Absolute.begin(), Absolute.end()), Absolute.end());
    size_t Offset = 0;
    for (size_t I = 0; I < Files.size(); ++I) {
      auto &F = Files[I]; F.Begin = Offset;
      while (Offset < Points.size() && Points[Offset] >> 32 == I) ++Offset;
      F.End = Offset;
      F.FirstOrdinary = std::upper_bound(Points.begin()+F.Begin, Points.begin()+F.End, (uint64_t(I)<<32)|1u) - Points.begin();
    }
    return Offset == Points.size();
  }

  std::optional<SourcePoint> point(const DIFile *F, unsigned Line) const {
    auto I = IDs.find(const_cast<DIFile *>(F));
    if (I == IDs.end()) return std::nullopt;
    unsigned FI = FileOfNode[I->second];
    if (FI >= Files.size()) return std::nullopt;
    auto &D = Files[FI]; uint64_t P = (uint64_t(FI)<<32) | Line;
    auto End = Points.begin()+D.End, At = std::lower_bound(Points.begin()+D.Begin, End, P);
    if (At == End || *At != P) return std::nullopt;
    unsigned Rank = Line < 2 ? Line : 2 + (At - (Points.begin()+D.FirstOrdinary));
    return SourcePoint{(uint64_t(FI)<<32) | Rank, Line};
  }
  std::optional<unsigned> rank(const DIFile *F, unsigned Line) const {
    if (auto Id = point(F, Line)) return static_cast<unsigned>(Id->first);
    return std::nullopt;
  }
  bool append(std::string &Wire) const {
    Wire.append("CGU-GLOBAL-LINES\x01", 17);
    appendInteger(Wire, Absolute.size()); unsigned Last = 0;
    for (unsigned Line : Absolute) { appendInteger(Wire, Line-Last); Last=Line; }
    appendInteger(Wire, Files.size());
    for (const auto &File : Files) {
      appendString(Wire, File.Name.first); appendString(Wire, File.Name.second);
      appendInteger(Wire, File.Check.first); appendString(Wire, File.Check.second);
      appendInteger(Wire, File.End-File.Begin); Last=0;
      for (size_t I=File.Begin; I<File.End; ++I) {
        unsigned Line=static_cast<uint32_t>(Points[I]);
        unsigned Rank=llvm::lower_bound(Absolute, Line)-Absolute.begin();
        appendInteger(Wire, Rank-Last); Last=Rank;
      }
    }
    return true;
  }
};


static bool supportedMode(Module &M) {
  if (M.getMaterializer() || !M.ifunc_empty() ||
      M.getModuleIdentifier() != M.getSourceFileName() ||
      M.getTargetTriple().str() != "x86_64-unknown-linux-gnu" ||
      !M.getModuleInlineAsm().empty() ||
      M.getDataLayoutStr() != "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-"
                              "i128:128-f80:128-n8:16:32:64-S128" ||
      PickMergedSourceLocations)
    return false;
  auto flag = [&](StringRef Name, unsigned Value) {
    auto *N = mdconst::dyn_extract_or_null<ConstantInt>(M.getModuleFlag(Name));
    return N && N->equalsInt(Value);
  };
  return flag("Dwarf Version", 4) && flag("Debug Info Version", 3);
}

static bool supportedAttributes(const Function &F) {
  for (Attribute A : F.getAttributes().getFnAttrs()) {
    if (A.isStringAttribute()) {
      StringRef K = A.getKindAsString(), V = A.getValueAsString();
      if (K == "target-cpu") {
        if (V != "x86-64")
          return false;
      } else if (K == "target-features") {
        if (V != "+sse,+sse2")
          return false;
      } else if (K != "probe-stack" && K != "alloc-family" &&
                 K != "alloc-variant-zeroed")
        return false;
    } else {
      StringRef K = Attribute::getNameFromAttrKind(A.getKindAsEnum());
      if (K.contains("sanitiz") || K.contains("profile") ||
          K.contains("instrument") || K == "naked")
        return false;
    }
  }
  return true;
}

class MetadataGraph {
  DenseMap<MDNode *, size_t> IDs;
  SmallVector<MDNode *, 128> Nodes;
  Placements Positions{IDs};

  void add(MDNode *N) {
    if (N && IDs.try_emplace(N, Nodes.size()).second)
      Nodes.push_back(N);
  }

  bool recordLine(DIFile *File, unsigned Line) {
    if (!File) return false;
    add(File);
    return Positions.recordLine(File, Line);
  }

  bool inspectNode(MDNode *N, unsigned &CompileUnits) {
    if (N->isTemporary())
      return false;
    if (auto *C = dyn_cast<DICompileUnit>(N)) {
      ++CompileUnits;
      return C->getEmissionKind() == DICompileUnit::LineTablesOnly &&
             !C->getDebugInfoForProfiling() &&
             C->getSplitDebugFilename().empty() && !C->getDWOId() &&
             C->getMacros().empty() && C->getGlobalVariables().empty() &&
             C->getRetainedTypes().empty();
    }
    if (auto *F = dyn_cast<DIFile>(N))
      return !F->getSource() && Positions.recordFile(F);
    if (auto *S = dyn_cast<DISubprogram>(N))
      return S->getRetainedNodes().empty() && S->getAnnotations().empty() &&
             !S->getKeyInstructionsEnabled() &&
             recordLine(S->getFile(), S->getLine()) &&
             recordLine(S->getFile(), S->getScopeLine());
    if (auto *L = dyn_cast<DILocation>(N))
      return recordLine(L->getFile(), L->getLine());
    if (auto *B = dyn_cast<DILexicalBlock>(N))
      return recordLine(B->getFile(), B->getLine());
    return isa<MDTuple, DINamespace, DILexicalBlockFile, DISubroutineType>(N);
  }

  TempMDNode copyNode(MDNode *N) const {
    auto &C = N->getContext();
    if (auto *F = dyn_cast<DIFile>(N)) {
      auto Check = F->getChecksum();
      SmallString<64> Zeros;
      if (Check) {
        Zeros.assign(Check->Value.size(), '0');
        Check->Value = Zeros;
      }
      return DIFile::getTemporary(C, F->getFilename(), F->getDirectory(), Check,
                                  F->getSource());
    }
    if (auto *S = dyn_cast<DISubprogram>(N)) {
      auto Line = Positions.rank(S->getFile(), S->getLine()),
           ScopeLine = Positions.rank(S->getFile(), S->getScopeLine());
      if (!Line || !ScopeLine)
        return {};
      return DISubprogram::getTemporary(
          C, S->getScope(), S->getName(), S->getLinkageName(), S->getFile(),
          *Line, S->getType(), *ScopeLine, S->getContainingType(),
          S->getVirtualIndex(), S->getThisAdjustment(), S->getFlags(),
          S->getSPFlags(), S->getUnit(), S->getTemplateParams(),
          S->getDeclaration(), S->getRetainedNodes(), S->getThrownTypes(),
          S->getAnnotations(), S->getTargetFuncName(),
          S->getKeyInstructionsEnabled());
    }
    if (auto *L = dyn_cast<DILocation>(N)) {
      auto Line = Positions.rank(L->getFile(), L->getLine());
      if (!Line)
        return {};
      return DILocation::getTemporary(C, *Line, L->getColumn(), L->getScope(),
                                      L->getInlinedAt(), L->isImplicitCode(),
                                      L->getAtomGroup(), L->getAtomRank());
    }
    if (auto *B = dyn_cast<DILexicalBlock>(N)) {
      auto Line = Positions.rank(B->getFile(), B->getLine());
      if (!Line)
        return {};
      return DILexicalBlock::getTemporary(C, B->getScope(), B->getFile(), *Line,
                                          B->getColumn());
    }
    return N->clone();
  }

public:
  bool collect(Module &M) {
    SmallVector<MDNode *, 64> Ordinary;
    for (auto &N : M.named_metadata()) {
      if (N.getName() != "llvm.dbg.cu" && N.getName() != "llvm.module.flags" &&
          N.getName() != "llvm.ident")
        return false;
      for (auto *Op : N.operands()) {
        add(Op);
        if (N.getName() != "llvm.dbg.cu")
          Ordinary.push_back(Op);
      }
    }
    SmallVector<Module::ModuleFlagEntry> Flags;
    M.getModuleFlagsMetadata(Flags);
    for (const auto &V : Flags) {
      StringRef K = V.Key->getString();
      static constexpr StringLiteral Known[] = {
          "Dwarf Version", "Debug Info Version", "PIC Level",
          "PIE Level",     "RtLibUseGOT",        "wchar_size",
          "uwtable",       "frame-pointer",      "override-stack-alignment",
          "MaxTLSAlign"};
      if (!llvm::is_contained(Known, K))
        return false;
    }
    SmallVector<std::pair<unsigned, MDNode *>, 8> Attached;
    auto visitAttached = [&](auto &V, bool Fn) {
      Attached.clear();
      V.getAllMetadata(Attached);
      for (const auto &[Kind, N] : Attached) {
        add(N);
        if (Kind != LLVMContext::MD_dbg)
          Ordinary.push_back(N);
        if (Kind == LLVMContext::MD_prof) {
          auto *Tag = N->getNumOperands()
                          ? dyn_cast_or_null<MDString>(N->getOperand(0))
                          : nullptr;
          if (Fn || !Tag || Tag->getString() != "branch_weights")
            return false;
        }
      }
      return true;
    };
    for (auto &GO : M.global_objects()) {
      StringRef N = GO.getName();
      if (N.starts_with("__llvm_prf") || N.starts_with("__llvm_cov") ||
          N.starts_with("__llvm_profile") || !visitAttached(GO, true))
        return false;
    }
    for (auto &F : M) {
      StringRef N = F.getName();
      if (N.starts_with("llvm.pseudoprobe") ||
          N.starts_with("llvm.instrprof") || N.starts_with("__llvm_profile") ||
          N.starts_with("__sanitizer") || N.starts_with("__asan_") ||
          N.starts_with("__msan_") || N.starts_with("__tsan_") ||
          N.starts_with("__ubsan_") || !supportedAttributes(F))
        return false;
      for (auto &B : F) {
        if (B.getTrailingDbgRecords() && !B.getTrailingDbgRecords()->empty())
          return false;
        for (auto &I : B) {
          if (I.hasDbgRecords() || !visitAttached(I, false))
            return false;
          if (auto *Call = dyn_cast<CallBase>(&I); Call && Call->isInlineAsm())
            return false;
          for (Value *V : I.operands())
            if (auto *MV = dyn_cast<MetadataAsValue>(V))
              if (auto *N = dyn_cast<MDNode>(MV->getMetadata())) {
                add(N);
                Ordinary.push_back(N);
              }
        }
      }
    }
    SmallPtrSet<MDNode *, 32> Seen;
    while (!Ordinary.empty()) {
      auto *N = Ordinary.pop_back_val();
      if (!Seen.insert(N).second)
        continue;
      if (isa<DINode, DILocation>(N))
        return false;
      for (auto &Op : N->operands())
        if (auto *Child = dyn_cast_or_null<MDNode>(Op.get()))
          Ordinary.push_back(Child);
    }
    unsigned CUs = 0;
    for (size_t I = 0; I < Nodes.size(); ++I) {
      auto *N = Nodes[I];
      if (!inspectNode(N, CUs))
        return false;
      for (auto &Op : N->operands())
        if (auto *Child = dyn_cast_or_null<MDNode>(Op.get()))
          add(Child);
    }
    return CUs == 1 && Positions.finish();
  }
  bool normalize(Module &M) const {
    std::vector<TempMDNode> Temp;
    std::vector<TrackingMDNodeRef> New;
    Temp.reserve(Nodes.size());
    New.reserve(Nodes.size());
    for (auto *N : Nodes) {
      auto T = copyNode(N);
      if (!T)
        return false;
      Temp.push_back(std::move(T));
      New.emplace_back(Temp.back().get());
    }
    for (auto &T : Temp) {
      for (unsigned I = 0, E = T->getNumOperands(); I < E; ++I)
        if (auto *N = dyn_cast_or_null<MDNode>(T->getOperand(I))) {
          auto P = IDs.find(N);
          if (P == IDs.end())
            return false;
          T->replaceOperandWith(I, New[P->second].get());
        }
    }
    for (unsigned I = 0; I < Nodes.size(); ++I) {
      MDNode *N = Nodes[I]->isDistinct()
                      ? MDNode::replaceWithDistinct(std::move(Temp[I]))
                      : MDNode::replaceWithUniqued(std::move(Temp[I]));
      New[I].reset(N);
    }
    for (auto &N : New)
      if (!N->isResolved())
        N->resolveCycles();
    auto mapped = [&](MDNode *N) -> MDNode * {
      auto Pos = IDs.find(N);
      return Pos == IDs.end() ? nullptr : New[Pos->second].get();
    };
    for (auto &N : M.named_metadata())
      for (unsigned I = 0; I < N.getNumOperands(); ++I) {
        auto *V = mapped(N.getOperand(I));
        if (!V)
          return false;
        N.setOperand(I, V);
      }
    SmallVector<std::pair<unsigned, MDNode *>, 8> A;
    for (auto &GO : M.global_objects()) {
      A.clear();
      GO.getAllMetadata(A);
      GO.clearMetadata();
      for (auto &KV : A) {
        auto *N = mapped(KV.second);
        if (!N)
          return false;
        GO.addMetadata(KV.first, *N);
      }
    }
    for (auto &F : M)
      for (auto &B : F)
        for (auto &I : B) {
          A.clear();
          I.getAllMetadata(A);
          for (auto &KV : A) {
            auto *N = mapped(KV.second);
            if (!N)
              return false;
            I.setMetadata(KV.first, N);
          }
          for (auto &O : I.operands())
            if (auto *V = dyn_cast<MetadataAsValue>(O.get()))
              if (auto *N = dyn_cast<MDNode>(V->getMetadata())) {
                auto *V = mapped(N);
                if (!V)
                  return false;
                O.set(MetadataAsValue::get(M.getContext(), V));
              }
        }
    return !verifyModule(M);
  }
  // Bind every possible local comparison, including lexical and inlined caller
  // scopes. Also bind the union with the preceding LLVM function. The latter
  // is a research guard whose physical emission assumptions are under audit.
  std::optional<std::array<uint8_t, 32>> functionOrder(const Module &M) const {
    SHA256 Hash; Hash.update("OHM-FUNCTION-AND-NEIGHBOR-ORDER-1");
    SmallVector<uint32_t, 0> Seen(Nodes.size(), 0);
    SmallVector<const MDNode *, 32> Pending;
    SmallVector<SourcePoint, 0> Here, Previous, Group;
    SmallVector<unsigned, 0> Lines;
    std::string Record; uint32_t Epoch = 0;
    for (const auto &F : M) {
      if (F.isDeclaration()) continue;
      if (++Epoch == 0) return std::nullopt;
      bool Good = true;
      auto visit = [&](const MDNode *N) {
        if (!N) return;
        auto I = IDs.find(const_cast<MDNode *>(N));
        if (I == IDs.end()) { Good=false; return; }
        if (Seen[I->second] == Epoch) return;
        Seen[I->second] = Epoch; Pending.push_back(N);
      };
      auto point = [&](const DIFile *File, unsigned Line) {
        if (auto P = Positions.point(File, Line)) Here.push_back(*P); else Good=false;
      };
      visit(F.getSubprogram());
      for (const auto &B : F) for (const auto &I : B) visit(I.getDebugLoc().get());
      Here.clear();
      while (Good && !Pending.empty()) {
        auto *N = Pending.pop_back_val();
        if (auto *L = dyn_cast<DILocation>(N)) {
          point(L->getFile(),L->getLine()); visit(L->getScope()); visit(L->getInlinedAt());
        } else if (auto *S = dyn_cast<DISubprogram>(N)) {
          point(S->getFile(),S->getLine()); point(S->getFile(),S->getScopeLine());
          visit(S->getScope()); visit(S->getDeclaration());
        } else if (auto *B = dyn_cast<DILexicalBlock>(N)) {
          point(B->getFile(),B->getLine()); visit(B->getScope());
        } else if (auto *B = dyn_cast<DILexicalBlockFile>(N)) {
          visit(B->getScope());
        } else if (auto *S = dyn_cast<DINamespace>(N)) {
          visit(S->getScope());
        } else if (!isa<DIFile, DICompileUnit>(N)) {
          Good=false;
        }
      }
      if (!Good) return std::nullopt;
      llvm::sort(Here); Here.erase(std::unique(Here.begin(),Here.end()),Here.end());
      Group.clear(); std::set_union(Previous.begin(),Previous.end(),Here.begin(),Here.end(),std::back_inserter(Group));
      Lines.clear(); Lines.push_back(0); Lines.push_back(1);
      for (auto &[ID, Line] : Group) Lines.push_back(Line);
      llvm::sort(Lines); Lines.erase(std::unique(Lines.begin(),Lines.end()),Lines.end());
      Record.clear(); appendString(Record,F.getName()); appendInteger(Record,Group.size());
      for (auto &[ID,Line] : Group) {
        appendInteger(Record,ID); appendInteger(Record,llvm::lower_bound(Lines,Line)-Lines.begin());
      }
      Hash.update(Record); Previous.swap(Here);
    }
    return Hash.final();
  }

  bool appendPlacements(std::string &Out) const {
    return Positions.append(Out);
  }
};

class BitcodeDigest final : public raw_ostream {
  SHA256 Hash;
  uint64_t Bytes = 0;
  void write_impl(const char *Data, size_t Length) override {
    Hash.update(StringRef(Data, Length));
    Bytes += Length;
  }
  uint64_t current_pos() const override { return Bytes; }

public:
  std::array<uint8_t, 32> finish() {
    flush();
    return Hash.final();
  }
};

static bool getSourcePlacement(Module &Original, std::string &Out) {
  if (!supportedMode(Original) || verifyModule(Original))
    return false;
  // This is the actual input to machine codegen, after the ordinary optimizer.
  // Preserve every value, operand and attribute while normalizing debug coordinates.
  auto Scratch = CloneModule(Original);
  MetadataGraph Graph;
  if (!Graph.collect(*Scratch)) return false;
  auto Order = Graph.functionOrder(*Scratch);
  if (!Order || !Graph.normalize(*Scratch))
    return false;
  BitcodeDigest Stream;
  WriteBitcodeToFile(*Scratch, Stream);
  auto Code = Stream.finish();
  SHA256 Combined;
  Combined.update("OHM-POST-O1-FILE-RANKED-BITCODE-AND-NEIGHBORS-3");
  Combined.update(Code); Combined.update(*Order);
  auto Key = Combined.final();
  Out.append("OSP\x03", 4);
  Out.append(reinterpret_cast<const char *>(Key.data()), Key.size());
  return Graph.appendPlacements(Out);
}
} // namespace

extern "C" bool LLVMRustGetSourcePlacement(LLVMModuleRef Module, void *Context,
                                           SourcePlacementCallback Emit) {
  if (!Module || !Emit)
    return false;
  std::string Wire;
  if (!getSourcePlacement(*unwrap(Module), Wire))
    return false;
  Emit(Context, reinterpret_cast<const uint8_t *>(Wire.data()), Wire.size());
  return true;
}
#else
extern "C" bool LLVMRustGetSourcePlacement(LLVMModuleRef, void *,
                                           SourcePlacementCallback) {
  return false;
}
#endif
