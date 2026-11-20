# Dependency provenance (author-generated from `Cargo.lock`; no network was used to write this file)

- `Cargo.lock` SHA-256: `cb0ab0844775b6ef33a87edc52101d83b1f735bed8ea1dbd061f91e905607ab1` (20887 bytes)
- Packages: 88 total = 1 local (`atm-mcp`) + 87 registry packages. Registry sources: registry+https://github.com/rust-lang/crates.io-index
- git sources: 0. path sources other than the local package: 0. Alternate registries: 0. Every registry package has a checksum in the lock.
- Cargo resolved this lock on 2026-11-19 with `cargo generate-lockfile` (crates.io sparse index only; no archives). One `cargo fetch --locked --target x86_64-pc-windows-msvc` then downloaded 19 archives.

## Reconciliation: 26 uncached candidates vs 19 fetched
26 lock packages were absent from the local cache before the fetch. 19 were downloaded. The other 7 are not needed for `x86_64-pc-windows-msvc` (they are target-specific: WebAssembly or non-Windows paths) and were therefore correctly not fetched:
`bumpalo` (via wasm-bindgen-macro-support), `cc` (via iana-time-zone-haiku), `js-sys`, `wasm-bindgen`, `wasm-bindgen-macro`, `wasm-bindgen-macro-support`, `wasm-bindgen-shared` (via iana-time-zone / uuid on wasm targets).
`cargo metadata --offline --locked --filter-platform x86_64-pc-windows-msvc` succeeds and needs 64 of the 87 registry packages; an unfiltered offline `cargo metadata` fails on `bumpalo`, which confirms those archives were not fetched.
**Consequence:** the cache now supports only the Windows x64 target. A host with another target (for example Linux) would miss some of those 7 crates offline and must STOP rather than fetch.
The 19 fetched: block-buffer, bytes, const-oid, crypto-common, digest, futures, hybrid-array, itoa, mio, pastey, pin-project-lite, rmcp, schemars_derive, serde_derive_internals, sha2, syn, tokio-macros, unicode-normalization, uuid.

## All registry packages (name, version, checksum, needed for windows-msvc)

| name | version | sha256 checksum | windows-msvc |
|---|---|---|---|
| android_system_properties | 0.1.6 | `ae221649c9976a6f6c56ae1facf410f3ddb33cc661c4b7b61020a912d4237fbc` | no |
| autocfg | 1.5.1 | `f2032f911046de80f0a198e0901378627c33f59ea0ac00e363d481118bd70a53` | yes |
| block-buffer | 0.12.1 | `d2f6c7dbe95a6ed67ad9f18e57daf93a2f034c524b99fd2b76d18fdfeb6660aa` | yes |
| bumpalo | 3.20.4 | `87a0cf66fd1eb2fb7b822cb78370d82303750809bc6ba855f3f58e517d3238db` | no |
| bytes | 1.12.2 | `776dfebd015e2d8732c684a2c71003674ccb49457b539b9e82151ba38e96dd7c` | yes |
| cc | 1.11.2 | `33ef9e3e8154a45c7a95cff4c1299702106ad94062d7bb85fdc5d3a5afd91d45` | no |
| cfg-if | 1.0.5 | `4e7648175b45a9a48536d676f68d918270699102aa8dab5496df06904c914600` | yes |
| chrono | 0.4.45 | `1aa79e62e7697b8e29b513a68abacf485adcd1fe8284a4316c5ae868e6633327` | yes |
| const-oid | 0.10.3 | `be95d4a884358a21a199689323c29e9256b4a1b315e22f183d155422041d3a9a` | yes |
| core-foundation-sys | 0.8.7 | `773648b94d0e5d620f64f280777445740e61fe701025087ec8b57f45c791888b` | no |
| cpufeatures | 0.3.1 | `5ca28b0ae3115b884660db4118d803791fd6756b6e88f39c0f3f7859060d7566` | yes |
| crypto-common | 0.2.2 | `ce6e4c961d6cd6c9a86db418387425e8bdeaf05b3c8bc1411e6dca4c252f1453` | yes |
| digest | 0.11.3 | `f1dd6dbb5841937940781866fa1281a1ff7bd3bf827091440879f9994983d5c2` | yes |
| dyn-clone | 1.0.20 | `d0881ea181b1df73ff77ffaaf9c7544ecc11e82fba9b5f27b262a3c73a332555` | yes |
| equivalent | 1.0.3 | `00d174d5400e5e8fd687ad1049e2f578285fa914201b1af7e8b112a4546bd826` | yes |
| find-msvc-tools | 0.1.15 | `1c66915cc5391097de7a2b0b972e8505aa33c0df401fd3a6811dbdb32881ba48` | no |
| futures | 0.3.34 | `9a31d2a3fbaaeb2af2368bbdd904aa8e812d3c04a1ee10d3171f52d556e5d0a3` | yes |
| futures-channel | 0.3.34 | `b1f9e3d69d39e4862ffed03ed071a76f9a13ba1d9109d355b0f0aa6b15e393c4` | yes |
| futures-core | 0.3.34 | `92d699e522242e69e3003b94ecc1f960f3a5e015aa7c5d7486e65ad01dd94f5e` | yes |
| futures-executor | 0.3.34 | `031b47cf1a3c6cc8bc2fc76cd437f521619387907d469316e7c0bc278f1f5432` | yes |
| futures-io | 0.3.34 | `53c0fa8157de1303bfffdaa1cc2a673bfffb60102f76b0ef4441659124373fed` | yes |
| futures-macro | 0.3.34 | `9fb9654ba8355388abeb8dcb4fc62f511300867002afc858860463bdd9fe0c44` | yes |
| futures-sink | 0.3.34 | `1944426bf7d03f1d14f708785e4b33efd750b36d48a157b836b3efc15ede8e1d` | yes |
| futures-task | 0.3.34 | `cd417de3d1d015fc3bfd2b1ea46dfc7bab72ef86f1cc7cc9c78e728b34a6d1fd` | yes |
| futures-util | 0.3.34 | `0d50a92467f8ba5dd6e3ee5d4bd04d73ab2e4e1c44474a0674821dfce14b79bc` | yes |
| getrandom | 0.4.3 | `300e883d756b2e4ec94e02791f39b04b522276138852cfc41d9fb7e904106099` | yes |
| hashbrown | 0.17.1 | `ed5909b6e89a2db4456e54cd5f673791d7eca6732202bbf2a9cc504fe2f9b84a` | yes |
| hybrid-array | 0.4.15 | `27f864f10dfb56725ce5ce5472bc52252c8f93a4ab86327122cebf62c5f59a17` | yes |
| iana-time-zone | 0.1.65 | `e31bc9ad994ba00e440a8aa5c9ef0ec67d5cb5e5cb0cc7f8b744a35b389cc470` | no |
| iana-time-zone-haiku | 0.1.2 | `f31827a206f56af32e590ba56d5d2d085f558508192593743f16b2306495269f` | no |
| indexmap | 2.14.2 | `cc4e190f5d26ca7051642629da2c52fc03bde85a03197c99408dcd291734c855` | yes |
| itoa | 1.0.19 | `3bd1574e617a9a6c1f8f8181bd7e841e1f20e4277607ef28945765188d6284d4` | yes |
| js-sys | 0.3.107 | `8dc09d2154098bbf4ef54eab6b117a3c24cbbffe94046b16ee6d6d4510584d3a` | no |
| libc | 0.2.191 | `65cd87c9a6d60536580b316ad985726104730a5151c99845f2490428ef91240a` | no |
| log | 0.4.35 | `5d15f86d12da44cd8d4ec3c73984537eb7f4cadcb9776a575a0a8a05ca11005c` | no |
| memchr | 2.8.3 | `cf8baf1c55e62ffcace7a9f06f4bd9cd3f0c4beb022d3b367256b91b87513d98` | yes |
| mio | 1.2.5 | `20521a903f02b0a573cd6840e9d0a5a0cc043d361fe924092cd70f8556cc9c4a` | yes |
| num-traits | 0.2.19 | `071dfc062690e90b734c0b2273ce72ad0ffa95f0c74596bc250dcfd960262841` | yes |
| once_cell | 1.21.4 | `9f7c3e4beb33f85d45ae3e3a1792185706c8e16d043238c593331cc7cd313b50` | yes |
| pastey | 0.2.3 | `2ee67f1008b1ba2321834326597b8e186293b049a023cdef258527550b9935b4` | yes |
| pin-project-lite | 0.2.18 | `c938015719552d665fd0553b90d96cd7ff58e83e4c061ca99e6bb78865a72752` | yes |
| proc-macro2 | 1.0.107 | `985e7ec9bb745e6ce6535b544d84d6cd6f7ad8bd711c398938ae983b91a766d9` | yes |
| quote | 1.0.47 | `1fbf4db142a473a8d80c26bbf18454ed458bf8d26c8219c331daecfdbd079001` | yes |
| r-efi | 6.0.0 | `f8dcc9c7d52a811697d2151c701e0d08956f92b0e24136cf4cf27b57a6a0d9bf` | no |
| ref-cast | 1.0.27 | `7e440fb4e4b4147295338efb76001ab9e4efc0e5839df2c47fc5ac2381d365c3` | yes |
| ref-cast-impl | 1.0.27 | `92ecd8964f8453721699a1ed72037b0db49ce2f5a5138486ee89bed6f67cdf3a` | yes |
| rmcp | 3.9.0 | `d24a7e92f4bcaa36b97fa93c4fa7ca8f3e669eced949345b3462dba333c7f909` | yes |
| rustversion | 1.0.23 | `cf54715a573b99ac80df0bc206da022bcd442c974952c7b9720069370852e21f` | no |
| schemars | 1.2.3 | `ab508826f74a77ca9d5aba6ff19b522583ee3eaf28a19384ff3d0e5835fadf6e` | yes |
| schemars_derive | 1.2.3 | `e1c3a92094fa7d61aa124645844facb6b554dfc797136d0f5fd1f890e2bffc69` | yes |
| serde | 1.0.229 | `4148590afebada386688f18773da617792bf2ef03ffc1e4cbd2b1d45b023e0ba` | yes |
| serde_core | 1.0.229 | `67dca2c9c51e58a4791a4b1ed58308b39c64224d349a935ab5039aa360942a48` | yes |
| serde_derive | 1.0.229 | `e7a5d71263a5a7d47b41f6b3f06ba276f10cc18b0931f1799f710578e2309348` | yes |
| serde_derive_internals | 0.30.0 | `f852137cce035d6a4df67ccce505ff6b3e9fd3a10e3e52b24dc71e650bb1a9bd` | yes |
| serde_json | 1.0.154 | `e7e9cc8b1b85264074fbcc02a88680c4096b1e47df8f739dceb03bf482f04bd6` | yes |
| sha2 | 0.11.1 | `47d7069beb7d6ac7b9acd1039986e73443f24234f41074da099d6f994ac9ad19` | yes |
| shlex | 2.0.1 | `f8fadd59c855ef2080decdef8ff161eb6661b86933c9d82e5ba29dc602a55aba` | no |
| slab | 0.4.12 | `0c790de23124f9ab44544d7ac05d60440adc586479ce501c1d6d7da3cd8c9cf5` | yes |
| socket2 | 0.6.5 | `c3d1e2c7f27f8d4cb10542a02c49005dbd6e93095799d6f3be745fae9f8fedd4` | yes |
| syn | 2.0.119 | `872831b642d1a07999a962a351ed35b955ea2cfc8f3862091e2a240a84f17297` | yes |
| syn | 3.0.10 | `4ae9ccbea3a7b0b09443036490c8f34a8b1b760fd345e5c0dae0aac718faea79` | yes |
| thiserror | 2.0.21 | `09e52cb86a36cede5cb101bf8908837b3e4c6e5e59fe7fd85c23fb56200d189e` | yes |
| thiserror-impl | 2.0.21 | `fe5197923287db20a58125f0bc85c062f7f2c892de97b18c356f9efb14b28524` | yes |
| tinyvec | 1.13.3 | `fd3ca314f692efd6c868f8408f53fe444634a845f96c028b97d35f6a1f79f0ee` | yes |
| tokio | 1.54.1 | `b52fd3255258d26fac7264ab5a76b53c13367136dc011d3d6ad4f8ed070396a3` | yes |
| tokio-macros | 2.7.2 | `78773a2a397f451582ce068015985c33193cf6dea8b74d2a639fe457b2f07b0e` | yes |
| tokio-util | 0.7.20 | `e464cf451ba96ebfc6f9b6542f17ee8b8956e33f1e40d9690624e59d7a7f8a4b` | yes |
| tracing | 0.1.44 | `63e71662fa4b2a2c3a26f570f037eb95bb1f85397f3cd8076caed2f026a6d100` | yes |
| tracing-attributes | 0.1.31 | `7490cfa5ec963746568740651ac6781f701c9c5ea257c58e057f3ba8cf69e8da` | yes |
| tracing-core | 0.1.36 | `db97caf9d906fbde555dd62fa95ddba9eecfd14cb388e4f491a66d74cd5fb79a` | yes |
| typenum | 1.20.1 | `b6f5e870be6c3b371b77fe0ee0bafb859fa4964b4404c27de1d380043c4dda20` | yes |
| unicode-ident | 1.0.27 | `a2c754d6c33795a1c324727428e5a7dedb5b06195f9890bdbcba760d3e246563` | yes |
| unicode-normalization | 0.1.26 | `75cd3fb03d5288b3ba9a1ee103ac7e9cbb59f0b03d64f8ddfcc04dd42758a85d` | yes |
| uuid | 1.29.3 | `3c14b15dffa2f4bd712103e857140aa04a4f2ff6e955c77d7eb8e53ee02a3b64` | yes |
| wasi | 0.11.1+wasi-snapshot-preview1 | `ccf3ec651a847eb01de73ccad15eb7d99f80485de043efb2f370cd654f4ea44b` | no |
| wasm-bindgen | 0.2.130 | `a0181c14ddd6c7b8b0b0132a5f1ea2f32be0d893cd3ea97b6e3b28886b8a3d1c` | no |
| wasm-bindgen-macro | 0.2.130 | `6e931a02a39abdbf66966fc35902b5ed58344750a70885bed4ebb3d8eaf28ed9` | no |
| wasm-bindgen-macro-support | 0.2.130 | `2ca4b54280e455bd831e33a6b8e734baf432fe63fbc35c42ac14ce67a043a53a` | no |
| wasm-bindgen-shared | 0.2.130 | `3b1f9c3798725016c6ee5f2f57869ec0831e8bbc0ab0310a86e787e97f41389e` | no |
| windows-core | 0.62.2 | `b8e83a14d34d0623b51dce9581199302a221863196a1dde71a7663a4c2be9deb` | no |
| windows-implement | 0.60.2 | `053e2e040ab57b9dc951b72c264860db7eb3b0200ba345b4e4c3b14f67855ddf` | no |
| windows-interface | 0.59.3 | `3f316c4a2570ba26bbec722032c4099d8c8bc095efccdc15688708623367e358` | no |
| windows-link | 0.2.1 | `f0805222e57f7521d6a62e36fa9163bc891acd422f971defe97d64e70d0a4fe5` | yes |
| windows-result | 0.4.1 | `7781fa89eaf60850ac3d2da7af8e5242a5ea78d1a11c49bf2910bb5a73853eb5` | no |
| windows-strings | 0.5.1 | `7837d08f69c77cf6b07689544538e017c1bfcf57e34b4c0ff58e6c2cd3b37091` | no |
| windows-sys | 0.61.2 | `ae137229bcbd6cdf0f7b80a31df61766145077ddf49416a728b02cb3921ff3fc` | yes |
| zmij | 1.0.23 | `29666d0abbfad1e3dc4dcf6144730dd3a3ab225bbbdac83319345b1b44ccfc1b` | yes |

## Local package
`atm-mcp 0.1.0` (path = this directory, no source).

## Not done
- Advisory audit: NOT_RUN (`cargo-audit`/`cargo-deny` not installed; a fresh install or database download would be a new network use). Known rmcp advisories (all in Streamable HTTP/OAuth code that this feature set does not compile) are listed in the Phase 0 report.
- License summary (from `cargo metadata`): 41 `MIT OR Apache-2.0`, 12 `MIT`, 6 `Apache-2.0 OR MIT`, 2 `(MIT OR Apache-2.0) AND Unicode-3.0`, 1 `Unlicense OR MIT`, 1 `Zlib OR Apache-2.0 OR MIT`, 1 `Apache-2.0`, plus the local package `PolyForm-Noncommercial-1.0.0`.
