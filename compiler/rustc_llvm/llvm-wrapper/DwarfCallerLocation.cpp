// Match rustc's typed static caller locations against a bound predecessor ELF.
// An unsupported LLVM provider, target, use or object layout declines reuse.
#include "LLVMWrapper.h"
#include "llvm-c/TargetMachine.h"

#if defined(__linux__) &&                                                      \
    __has_include(                                                             \
        "llvm/Bitcode/OhmDebugObjectSignature.h") &&                           \
        __has_include(                                                         \
            "../../../src/llvm-project/llvm/lib/Bitcode/Writer/OhmCallerLocationProof.h")
#include "../../../src/llvm-project/llvm/lib/Bitcode/Writer/OhmCallerLocationProof.h"
#include "llvm-c/Core.h"
#include "llvm/Bitcode/OhmDebugObjectSignature.h"
#include "llvm/DebugInfo/DWARF/OhmRuntimeLocations.h"
#include "llvm/IR/ClosedOptimizerContext.h"
#include "llvm/IR/Module.h"
#include "llvm/Support/CBindingWrapping.h"
#include "llvm/Support/JSON.h"
#include "llvm/Target/TargetMachine.h"
#if __has_include("llvm/CodeGen/OhmEmittedGlobals.h")
#define OHM_EMITTED_GLOBALS
#include "llvm/CodeGen/AsmPrinter.h"
#include "llvm/CodeGen/OhmEmittedGlobals.h"
#include "llvm/MC/MCSymbol.h"
#endif
#include <array>
#include <cstring>
#include <elf.h>
#include <set>

using llvm::StringRef;
namespace {
bool returnOwner(llvm::Module *M, const char *File, uint32_t Line, char *Out,
                 size_t Capacity) {
  llvm::StringRef Owner;
  for (const llvm::GlobalVariable &G : M->globals()) {
    auto R = llvm::ohm_location::readRecord(G);
    if (!R || R->HasColumn || R->Path != File)
      continue;
    if (llvm::support::endian::read32le(R->Tail->getAsString().data() + 8) !=
        Line)
      continue;
    if (!Owner.empty() || G.hasName() || R->File->hasName() ||
        !G.getInitializer()->hasOneUse() ||
        G.getInitializer()->isUsedByMetadata() ||
        !llvm::ohm_location::returnedAddress(G))
      return false;
    const auto *Ret = llvm::cast<llvm::ReturnInst>(*G.user_begin());
    Owner = Ret->getFunction()->getName();
    if (!Owner.starts_with("_R") || !Owner.size() || Owner.size() >= Capacity)
      return false;
  }
  if (Owner.empty())
    return false;
  std::memcpy(Out, Owner.data(), Owner.size());
  Out[Owner.size()] = 0;
  return true;
}

struct Elf {
  StringRef bytes;
  Elf64_Ehdr h{};
  std::vector<Elf64_Shdr> sec;
  template <class T> bool read(StringRef data, uint64_t at, T &value) const {
    if (at > data.size() || sizeof(T) > data.size() - at)
      return false;
    std::memcpy(&value, data.data() + at, sizeof(T));
    return true;
  }
  bool init() {
    if (!read(bytes, 0, h) || std::memcmp(h.e_ident, "\177ELF\2\1\1", 7) ||
        h.e_type != ET_REL || h.e_machine != EM_X86_64 ||
        h.e_shentsize != sizeof(Elf64_Shdr) || !h.e_shnum ||
        h.e_shstrndx >= h.e_shnum || h.e_shoff > bytes.size())
      return false;
    for (unsigned i = 0; i < h.e_shnum; ++i) {
      Elf64_Shdr hdr;
      if (!read(bytes, h.e_shoff + i * sizeof(hdr), hdr) ||
          hdr.sh_offset > bytes.size() ||
          (hdr.sh_type != SHT_NOBITS &&
           hdr.sh_size > bytes.size() - hdr.sh_offset))
        return false;
      sec.push_back(hdr);
    }
    return sec[h.e_shstrndx].sh_type == SHT_STRTAB;
  }
  StringRef data(unsigned i) const {
    return bytes.substr(sec[i].sh_offset, sec[i].sh_size);
  }
  std::optional<StringRef> str(unsigned i, uint64_t off) const {
    if (i >= sec.size() || sec[i].sh_type != SHT_STRTAB ||
        off >= sec[i].sh_size)
      return {};
    auto v = data(i).drop_front(off);
    auto p = v.find('\0');
    if (p == StringRef::npos)
      return {};
    return v.take_front(p);
  }
  std::optional<StringRef> name(unsigned i) const {
    if (i >= sec.size())
      return {};
    return str(h.e_shstrndx, sec[i].sh_name);
  }
  std::optional<unsigned> unique(StringRef n) const {
    std::optional<unsigned> found;
    for (unsigned i = 0; i < sec.size(); ++i) {
      auto nm = name(i);
      if (!nm)
        return {};
      if (*nm == n) {
        if (found)
          return {};
        found = i;
      }
    }
    return found;
  }
  bool sym(unsigned table, uint64_t i, Elf64_Sym &s) const {
    if (table >= sec.size() || sec[table].sh_type != SHT_SYMTAB ||
        sec[table].sh_entsize != sizeof(s) ||
        i >= sec[table].sh_size / sizeof(s))
      return false;
    return read(data(table), i * sizeof(s), s);
  }
};
bool privateName(StringRef n) {
  if (!n.consume_front("anon.") || n.size() < 34)
    return false;
  for (char ch : n.take_front(32))
    if (!((ch >= 'a' && ch <= 'f') || (ch >= '0' && ch <= '9')))
      return false;
  n = n.drop_front(32);
  if (!n.consume_front(".") || n.empty())
    return false;
  for (char ch : n)
    if (ch < '0' || ch > '9')
      return false;
  return true;
}
#ifdef OHM_EMITTED_GLOBALS
using LocationKey = std::tuple<std::string, uint32_t, uint32_t>;
using NativeOwner = std::pair<std::string, std::string>;

struct EmittedNames {
  std::map<LocationKey, NativeOwner> names;
  bool seen = false;
  bool invalid = false;
  llvm::ohm::ScopedEmittedGlobals observer;

  explicit EmittedNames(const llvm::Module &module)
      : observer(module, capture, this) {}

  static void capture(void *context, const llvm::AsmPrinter &printer,
                      const llvm::Module &module) {
    auto &self = *static_cast<EmittedNames *>(context);
    if (self.seen) {
      self.invalid = true;
      return;
    }
    self.seen = true;
    for (const auto &g : module.globals()) {
      auto r = llvm::ohm_location::readRecord(g);
      if (!r || !r->HasColumn)
        continue;
      auto *symbol = printer.getSymbol(&g);
      auto *file = printer.getSymbol(r->File);
      if (!symbol->isDefined() || !file->isDefined())
        continue;
      auto global = symbol->getName(), filename = file->getName();
      if (!global.consume_front(".L") || !filename.consume_front(".L") ||
          !privateName(global) || !privateName(filename))
        continue;
      auto data = r->Tail->getAsString();
      LocationKey key(r->Path.str(),
                      llvm::support::endian::read32le(data.data() + 8),
                      llvm::support::endian::read32le(data.data() + 12));
      auto [it, inserted] = self.names.try_emplace(
          std::move(key), NativeOwner(global.str(), filename.str()));
      if (!inserted)
        it->second = {}; // A coalesced/ambiguous address cannot prove identity.
      if (self.names.size() > 65536) {
        self.invalid = true;
        return;
      }
    }
  }
};
#endif

struct Ref {
  unsigned section;
  Elf64_Rela r;
  Elf64_Sym target;
  unsigned symtab;
};
bool resolve(StringRef binary, StringRef owner, StringRef &record,
             StringRef &filepath) {
  if (!owner.starts_with("_R"))
    return false;
  for (char c : owner)
    if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
          (c >= '0' && c <= '9') || c == '_'))
      return false;
  Elf e{binary};
  if (!e.init())
    return false;
  auto code = e.unique((".text." + owner).str());
  if (!code)
    return false;
  auto s = e.sec[*code];
  if (s.sh_type != SHT_PROGBITS || (s.sh_flags & 6) != 6 ||
      e.data(*code) != StringRef("\x48\x8d\x05\x00\x00\x00\x00\xc3", 8))
    return false;
  unsigned ownerCount = 0;
  std::vector<Ref> refs;
  std::optional<Ref> edge;
  for (unsigned i = 0; i < e.sec.size(); ++i) {
    auto sh = e.sec[i];
    if (sh.sh_type == SHT_SYMTAB) {
      if (sh.sh_entsize != 24 || sh.sh_size % 24 || sh.sh_link >= e.sec.size())
        return false;
      for (uint64_t k = 0; k < sh.sh_size / 24; ++k) {
        Elf64_Sym sy;
        if (!e.sym(i, k, sy) || sy.st_shndx == SHN_XINDEX)
          return false;
        if (sy.st_shndx == *code && ELF64_ST_TYPE(sy.st_info) != STT_SECTION) {
          auto n = e.str(sh.sh_link, sy.st_name);
          if (!n || *n != owner || sy.st_info != STT_FUNC || sy.st_value ||
              sy.st_size != 8)
            return false;
          ++ownerCount;
        }
      }
    }
    if (sh.sh_type == SHT_REL || sh.sh_type == SHT_DYNSYM ||
        sh.sh_type == SHT_SYMTAB_SHNDX)
      return false;
    if (sh.sh_type != SHT_RELA)
      continue;
    if (sh.sh_entsize != 24 || sh.sh_size % 24 || sh.sh_info >= e.sec.size())
      return false;
    for (size_t j = 0; j < sh.sh_size; j += 24) {
      Ref t;
      t.section = sh.sh_info;
      t.symtab = sh.sh_link;
      if (!e.read(e.data(i), j, t.r) ||
          !e.sym(t.symtab, ELF64_R_SYM(t.r.r_info), t.target))
        return false;
      refs.push_back(t);
      if (t.section == *code) {
        if (edge)
          return false;
        edge = t;
      }
    }
  }
  auto unnamed = [&](const Ref &t) {
    return t.target.st_shndx < e.sec.size() &&
           t.target.st_info == STT_SECTION && !t.target.st_value &&
           e.str(e.sec[t.symtab].sh_link, t.target.st_name) ==
               std::optional<StringRef>(StringRef());
  };
  if (ownerCount != 1 || !edge || edge->r.r_offset != 3 ||
      ELF64_R_TYPE(edge->r.r_info) != R_X86_64_PC32 || edge->r.r_addend != -4 ||
      !unnamed(*edge))
    return false;
  auto idx = edge->target.st_shndx;
  unsigned in = 0;
  std::optional<Ref> filename;
  for (auto &t : refs) {
    if (t.target.st_shndx == idx)
      ++in;
    if (t.section == idx) {
      if (filename)
        return false;
      filename = t;
    }
  }
  if (in != 1 || !filename || filename->r.r_offset ||
      ELF64_R_TYPE(filename->r.r_info) != R_X86_64_64 || filename->r.r_addend ||
      !unnamed(*filename))
    return false;
  auto rn = e.name(idx), fn = e.name(filename->target.st_shndx);
  if (!rn || !fn || e.unique(*rn) != idx ||
      e.unique(*fn) != filename->target.st_shndx)
    return false;
  record = *rn;
  filepath = *fn;
  if (!record.consume_front(".data.rel.ro..L") ||
      !filepath.consume_front(".rodata..L"))
    return false;
  return privateName(record) && privateName(filepath);
}
struct Check {
  llvm::Module *current;
  StringRef roster;
  StringRef mapping;
  StringRef native;
  LLVMMemoryBufferRef result = nullptr;
  unsigned calls = 0;
  int check(StringRef raw) {
    if (++calls != 1)
      return 0;
    auto proposed = llvm::json::parse(raw);
    auto enrolled = llvm::json::parse(roster);
    if (!proposed || !enrolled) {
      if (!proposed)
        llvm::consumeError(proposed.takeError());
      if (!enrolled)
        llvm::consumeError(enrolled.takeError());
      return 0;
    }
    const auto *legacy = enrolled->getAsArray();
    const llvm::json::Array *named = nullptr;
    if (!legacy) {
      const auto *v = enrolled->getAsObject();
      if (!v || v->getInteger("version") != 2 ||
          !(legacy = v->getArray("returned")) ||
          !(named = v->getArray("native")) || named->size() > 65536)
        return 0;
    }
    std::map<std::tuple<StringRef, uint32_t, uint32_t>,
             std::pair<StringRef, StringRef>> positions;
    if (named) {
      for (const auto &v : *named) {
        const auto *row = v.getAsArray();
        if (!row || row->size() != 5)
          return 0;
        auto path = (*row)[0].getAsString(), global = (*row)[3].getAsString(),
             filename = (*row)[4].getAsString();
        auto line = (*row)[1].getAsInteger(), col = (*row)[2].getAsInteger();
        if (!path || !line || *line <= 0 || *line > UINT32_MAX || !col ||
            *col < 0 || *col > UINT32_MAX || !global || !filename ||
            !privateName(*global) || !privateName(*filename) ||
            !positions.emplace(std::make_tuple(*path, *line, *col),
                               std::make_pair(*global, *filename)).second)
          return 0;
      }
    }
    auto *rows = proposed->getAsArray();
    if (!rows || rows->empty() || rows->size() > 65536)
      return 0;
    for (auto &row : *rows) {
      auto *o = row.getAsObject();
      if (!o)
        return 0;
      auto global = o->getString("global");
      if (!global)
        return 0;
      if (!global->empty())
        continue;
      auto file = o->getString("file"), fileGlobal = o->getString("file_global");
      auto before = o->getInteger("old_line"), after = o->getInteger("new_line");
      if (!file || !before || *before < 1 || *before > UINT32_MAX || !after ||
          *after < 1 || *after > UINT32_MAX || !fileGlobal || !fileGlobal->empty())
        return 0;
      StringRef record, filename;
      if (auto col = o->getInteger("column")) {
        if (*col < 0 || *col > UINT32_MAX)
          return 0;
        auto it = positions.find(std::make_tuple(*file, *before, *col));
        if (it == positions.end())
          return 0;
        std::tie(record, filename) = it->second;
      } else {
        const auto *column = o->get("column");
        if (!column || column->kind() != llvm::json::Value::Null)
          return 0;
        std::optional<StringRef> owner;
        for (const auto &r : *legacy) {
          const auto *x = r.getAsObject();
          if (!x)
            return 0;
          if (x->getString("file") == file && x->getInteger("line") == before) {
            if (owner || !(owner = x->getString("owner")))
              return 0;
          }
        }
        char now[4096];
        if (!owner || !returnOwner(current, file->str().c_str(), *after, now,
                                    sizeof(now)) || *owner != now ||
            !resolve(native, *owner, record, filename))
          return 0;
      }
      (*o)["global"] = record.str();
      (*o)["file_global"] = filename.str();
    }
    std::string fixed;
    llvm::raw_string_ostream(fixed) << *proposed;
    const char *reason = nullptr;
    return ohm_debug_rebase_locations_v1(mapping.data(), mapping.size(),
                                         fixed.data(), fixed.size(), native.data(),
                                         native.size(), &result, &reason) == 0 &&
           result;
  }

};
int callback(void *ctx, const char *p, size_t n) {
  return static_cast<Check *>(ctx)->check(StringRef(p, n));
}
bool returnedOwners(llvm::Module *M, char **result, size_t *len) {
  llvm::json::Array arr;
  std::set<std::pair<std::string, uint32_t>> seen;
  for (auto &g : M->globals()) {
    auto rec = llvm::ohm_location::readRecord(g);
    if (!rec || rec->HasColumn || !llvm::ohm_location::returnedAddress(g))
      continue;
    auto line =
        llvm::support::endian::read32le(rec->Tail->getAsString().data() + 8);
    auto path = rec->Path.str();
    if (!seen.emplace(path, line).second)
      continue;
    char owner[4096];
    if (returnOwner(M, path.c_str(), line, owner, sizeof(owner)))
      arr.push_back(llvm::json::Object{{"file", path},
                                       {"line", int64_t(line)},
                                       {"owner", std::string(owner)}});
  }
  std::string s;
  llvm::raw_string_ostream(s) << llvm::json::Value(std::move(arr));
  auto bytes = static_cast<char *>(std::malloc(s.size()));
  if (!bytes)
    return false;
  std::memcpy(bytes, s.data(), s.size());
  *result = bytes;
  *len = s.size();
  return true;
}

using SourceKey = std::pair<std::string, std::string>;
std::set<SourceKey> sourceKeys(const llvm::Module &M) {
  std::set<SourceKey> Keys;
  auto *CUs = M.getNamedMetadata("llvm.dbg.cu");
  if (!CUs || CUs->getNumOperands() != 1)
    return Keys;
  auto *Unit = llvm::dyn_cast<llvm::DICompileUnit>(CUs->getOperand(0));
  if (!Unit || Unit->getEmissionKind() != llvm::DICompileUnit::LineTablesOnly)
    return Keys;
  for (const auto &G : M.globals()) {
    auto R = llvm::ohm_location::readRecord(G);
    if (!R || !G.getInitializer()->hasOneUse() ||
        G.getInitializer()->isUsedByMetadata())
      continue;
    bool Usable = R->HasColumn ? G.hasName() == R->File->hasName() &&
                                     llvm::ohm_location::addressOnly(G)
                               : llvm::ohm_location::returnedAddress(G);
    if (Usable)
      Keys.emplace(Unit->getDirectory().str(), R->Path.str());
  }
  return Keys;
}

void append(std::string &Out, StringRef In) {
  char Size[4];
  llvm::support::endian::write32le(Size, In.size());
  Out.append(Size, sizeof(Size));
  Out.append(In.data(), In.size());
}

// Avoid running the full writer when none of the typed runtime records can
// be affected by a verified source move. A positive still needs the full proof.
bool canMove(const llvm::Module &M, StringRef Mapping) {
  auto Parsed = llvm::json::parse(Mapping);
  if (!Parsed) {
    llvm::consumeError(Parsed.takeError());
    return false;
  }
  auto *Map = Parsed->getAsObject();
  if (!Map)
    return false;
  auto *Files = Map->getArray("files");
  if (!Files)
    return false;
  std::map<std::string, std::set<uint32_t>> Changed;
  for (auto &F : *Files) {
    auto *File = F.getAsObject();
    if (!File)
      return false;
    auto Name = File->getString("filename");
    auto *Lines = File->getArray("lines");
    if (!Name || !Lines)
      return false;
    for (auto &Pair : *Lines) {
      auto *P = Pair.getAsArray();
      if (!P || P->size() != 2)
        return false;
      auto Old = (*P)[0].getAsInteger(), New = (*P)[1].getAsInteger();
      if (!Old || !New || *Old < 1 || *New < 1 || *New > UINT32_MAX)
        return false;
      if (*Old != *New)
        Changed[Name->str()].insert(*New);
    }
  }
  for (const auto &G : M.globals()) {
    auto R = llvm::ohm_location::readRecord(G);
    if (!R)
      continue;
    auto File = Changed.find(R->Path.str());
    if (File == Changed.end())
      continue;
    if (File->second.count(
            llvm::support::endian::read32le(R->Tail->getAsString().data() + 8)))
      return true;
  }
  return false;
}
} // namespace

extern "C" bool LLVMRustDwarfTypedShape(LLVMModuleRef Ref,
                                        LLVMTargetMachineRef TM,
                                        uint64_t *Shape) {
  auto *T = reinterpret_cast<const llvm::TargetMachine *>(TM);
  const auto &M = *llvm::unwrap(Ref);
  if (!T || T->getTargetCPU() != "x86-64" ||
      !T->getTargetFeatureString().empty() ||
      T->getTargetTriple().getArch() != llvm::Triple::x86_64 ||
      !T->getTargetTriple().isOSLinux() ||
      !T->getTargetTriple().isOSBinFormatELF() ||
      T->getRelocationModel() != llvm::Reloc::PIC_ ||
      T->getCodeModel() != llvm::CodeModel::Small ||
      T->getOptLevel() != llvm::CodeGenOptLevel::Less ||
      !M.getDataLayout().isLittleEndian() ||
      M.getDataLayout().getPointerSizeInBits() != 64)
    return false;
  uint64_t H = 14695981039346656037ULL;
  auto Add = [&](uint64_t X) { H = (H ^ X) * 1099511628211ULL; };
  auto Node = [&](const llvm::GlobalValue &G) {
    auto Name = G.getName();
    Add(Name.size());
    for (uint8_t B : Name.bytes())
      Add(B);
    Add(G.getLinkage());
    Add(G.isDeclaration());
  };
  for (const auto &G : M.globals())
    Node(G);
  for (const auto &F : M)
    Node(F);
  *Shape = H;
  return true;
}

extern "C" bool LLVMRustDwarfTypedContext(LLVMModuleRef M, uint8_t *Digest) {
  std::array<uint8_t, 32> Key;
  if (!llvm::getClosedOptimizerContextV1(*llvm::unwrap(M), Key))
    return false;
  std::memcpy(Digest, Key.data(), Key.size());
  return true;
}

extern "C" LLVMRustBuffer *LLVMRustDwarfTypedSnapshot(LLVMModuleRef Ref) {
  auto *M = llvm::unwrap(Ref);
  auto Files = sourceKeys(*M);
  if (Files.empty())
    return nullptr;
  std::array<uint8_t, 32> Context;
  if (!llvm::getClosedOptimizerContextV1(*M, Context))
    return nullptr;
  char *Saved = nullptr;
  size_t N = 0;
  if (!ohm_bc_save_v1(Ref, &Saved, &N) || !Saved)
    return nullptr;
  std::unique_ptr<char, decltype(&ohm_bc_free_v1)> Ordered(Saved,
                                                           ohm_bc_free_v1);
  if (!N || N > 128u << 20)
    return nullptr;
  char *Names = nullptr;
  size_t Count = 0;
  if (!returnedOwners(M, &Names, &Count))
    return nullptr;
  std::unique_ptr<char, decltype(&std::free)> Owners(Names, std::free);
  if (Count > 128u << 20)
    return nullptr;
  auto Out = std::make_unique<LLVMRustBuffer>();
  Out->data.append(reinterpret_cast<const char *>(Context.data()),
                   Context.size());
  append(Out->data, StringRef(Ordered.get(), N));
  append(Out->data, StringRef(Owners.get(), Count));
  for (const auto &[Dir, File] : Files) {
    append(Out->data, Dir);
    append(Out->data, File);
  }
  return Out.release();
}

extern "C" LLVMRustBuffer *
LLVMRustDwarfTypedMatch(LLVMModuleRef M, const char *Old, size_t N,
                        const char *Owners, size_t NOwners, const char *Map,
                        size_t NMap, const char *Proof, size_t NProof,
                        const char *Binary, size_t NBinary) {
  if (!canMove(*llvm::unwrap(M), {Map, NMap}))
    return nullptr;
  Check C{llvm::unwrap(M), {Owners, NOwners}, {Map, NMap}, {Binary, NBinary}};
  int Code = ohm_bc_match_relocatable_v1(M, Old, N, Map, NMap, Proof, NProof,
                                         callback, &C);
  if (!C.result)
    return nullptr;
  std::unique_ptr<LLVMOpaqueMemoryBuffer, decltype(&LLVMDisposeMemoryBuffer)>
      Buf(C.result, LLVMDisposeMemoryBuffer);
  const size_t OutputSize = LLVMGetBufferSize(Buf.get());
  if (Code != 2 || C.calls != 1 || !OutputSize || OutputSize > 128u << 20)
    return nullptr;
  auto Out = std::make_unique<LLVMRustBuffer>();
  Out->data.assign(LLVMGetBufferStart(Buf.get()), OutputSize);
  return Out.release();
}
#else
extern "C" bool LLVMRustDwarfTypedShape(LLVMModuleRef, LLVMTargetMachineRef,
                                        uint64_t *) {
  return false;
}
extern "C" bool LLVMRustDwarfTypedContext(LLVMModuleRef, uint8_t *) {
  return false;
}
extern "C" LLVMRustBuffer *LLVMRustDwarfTypedSnapshot(LLVMModuleRef) {
  return nullptr;
}
extern "C" LLVMRustBuffer *LLVMRustDwarfTypedMatch(LLVMModuleRef, const char *,
                                                   size_t, const char *, size_t,
                                                   const char *, size_t,
                                                   const char *, size_t,
                                                   const char *, size_t) {
  return nullptr;
}
#endif

extern "C" void *LLVMRustDwarfStartEmittedNames(LLVMModuleRef m) {
#ifdef OHM_EMITTED_GLOBALS
  return new EmittedNames(*llvm::unwrap(m));
#else
  return nullptr;
#endif
}

extern "C" void LLVMRustDwarfDropEmittedNames(void *capture) {
#ifdef OHM_EMITTED_GLOBALS
  delete static_cast<EmittedNames *>(capture);
#endif
}

extern "C" LLVMRustBuffer *LLVMRustDwarfFinishEmittedNames(
    void *capture, const char *returned, size_t size) {
#ifdef OHM_EMITTED_GLOBALS
  const auto &names = *static_cast<EmittedNames *>(capture);
  if (!names.seen || names.invalid || names.names.empty())
    return nullptr;
  auto old = llvm::json::parse(StringRef(returned, size));
  if (!old) {
    llvm::consumeError(old.takeError());
    return nullptr;
  }
  if (!old->getAsArray())
    return nullptr;
  llvm::json::Array native;
  for (const auto &[key, pair] : names.names) {
    if (pair.first.empty())
      continue;
    const auto &[path, line, column] = key;
    native.emplace_back(llvm::json::Array{
        path, int64_t(line), int64_t(column), pair.first, pair.second});
  }
  if (native.empty())
    return nullptr;
  auto out = std::make_unique<LLVMRustBuffer>();
  llvm::raw_string_ostream(out->data) << llvm::json::Value(llvm::json::Object{
      {"version", 2}, {"returned", std::move(*old)}, {"native", std::move(native)}});
  return out.release();
#else
  return nullptr;
#endif
}
