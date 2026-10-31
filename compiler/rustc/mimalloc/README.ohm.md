Mimalloc 3.5.1, from Microsoft commit `34fbd7e7cd4627424490afe19b20f8066bfc537d`.

Source: https://github.com/microsoft/mimalloc/tree/34fbd7e7cd4627424490afe19b20f8066bfc537d

The CMake build files, `include`, and `src` directories are copied without changes. The upstream MIT license is in `LICENSE`.

The `mimalloc` feature links this static allocator into the Linux GNU rustc executable, including allocations made by its LLVM library. The native malloc override is confined to rustc; rustdoc, Clippy, and standalone LLVM tools retain their configured allocators.
