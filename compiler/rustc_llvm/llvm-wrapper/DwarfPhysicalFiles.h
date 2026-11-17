// Source identities accompanying a successfully closed LLVM line certificate.
#include "llvm/ADT/SmallPtrSet.h"
#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/Module.h"
#include "llvm/Support/JSON.h"
#include <map>
#include <set>
#include <tuple>

namespace ohm_installed_dwarf {
inline bool physicalFiles(const llvm::Module &M, llvm::raw_ostream &OS) {
  using namespace llvm;
  SmallVector<const Metadata *, 128> Pending;
  SmallPtrSet<const Metadata *, 32> Seen;
  auto Add = [&](const Metadata *MD) {
    if (MD && Seen.insert(MD).second)
      Pending.push_back(MD);
  };
  for (const NamedMDNode &Named : M.named_metadata())
    for (const MDNode *Node : Named.operands())
      Add(Node);
  SmallVector<std::pair<unsigned, MDNode *>, 8> Attachments;
  auto Visit = [&](const auto &Object) {
    if (!Object.hasMetadata())
      return;
    Attachments.clear();
    Object.getAllMetadata(Attachments);
    for (const auto &Field : Attachments)
      Add(Field.second);
  };
  for (const auto &G : M.globals())
    Visit(G);
  for (const auto &G : M.ifuncs())
    Visit(G);
  for (const auto &F : M) {
    Visit(F);
    for (const auto &BB : F) {
      if (const_cast<BasicBlock &>(BB).getTrailingDbgRecords())
        return false;
      for (const auto &I : BB) {
        if (I.hasDbgRecords())
          return false;
        Visit(I);
        for (const auto &Op : I.operands())
          if (auto *V = dyn_cast_or_null<MetadataAsValue>(Op.get()))
            Add(V->getMetadata());
      }
    }
  }
  using Entry = std::tuple<std::string, std::string, unsigned, std::string>;
  std::set<Entry> Files;
  while (!Pending.empty()) {
    const auto *Node = dyn_cast<MDNode>(Pending.pop_back_val());
    if (!Node)
      continue;
    if (const auto *File = dyn_cast<DIFile>(Node)) {
      const auto Digest = File->getChecksum();
      Files.emplace(File->getDirectory().str(), File->getFilename().str(),
                    Digest ? static_cast<unsigned>(Digest->Kind) : 0,
                    Digest ? Digest->Value.str() : std::string());
      if (Files.size() > 65535)
        return false;
    }
    for (const MDOperand &Op : Node->operands())
      Add(Op.get());
  }
  json::Array Array;
  for (const auto &[Directory, Filename, Kind, Digest] : Files) {
    if (StringRef(Directory).contains('\0') || StringRef(Filename).contains('\0') ||
        !json::isUTF8(Directory) || !json::isUTF8(Filename))
      return false;
    Array.push_back(json::Array{Directory, Filename, int64_t(Kind), Digest});
  }
  OS << json::Value(std::move(Array));
  return !Files.empty();
}
} // namespace ohm_installed_dwarf
