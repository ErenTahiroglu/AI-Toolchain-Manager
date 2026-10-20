<!-- Verbatim body of GitHub Issue #24 (https://github.com/ErenTahiroglu/AI-Toolchain-Manager/issues/24), opened by ErenTahiroglu at 2026-10-20T21:46:09Z, captured on 2026-10-21 via the GitHub API. LF line endings. -->

# B3 authorization scope: persistent feature unlock is not a one-time jq installation

## B2 Red Team — one-time authorization vs durable B3 unlock
**Reviewed:** 1e2fe043b28a94ad113f78913f4c83ed2b725e5a, exact-SHA CI 39493167158 Windows/macOS PASS. B2 implementation is otherwise accepted. Real jq remains locked via B3_UNLOCK=None; no real byte access or installation approved.

### Finding
The proposed B3 changes `jq_pilot::B3_UNLOCK` from `None` to `Some(exact_recipe_digest)` in a separately built jq-pilot binary. Once enabled, `sources_for_app` installs the real `RetainedFileProvider` and one `PilotRecipe` gate on *every application startup*. The per-preview `ConsentBook` token is single-use, **but the feature unlock is not one-time**. The existing synthetic whole-flow B2 test explicitly demonstrates uninstall and reinstall under the same persistent pilot supply. A one-time installation authorization from the user must not be described as a persistently enforced one-use capability.

### Narrow required disposition before B3 real copy
Choose and test an accurate one-use pilot rule, without Windows Hello or broad new authorization architecture: bound to one exact artifact/recipe, one isolated pilot data directory and one successful install. Use manager-owned durable receipt/journal state so restart or remove-and-reinstall cannot silently reset a consumed pilot. A separate explicitly user-authorized removal may be supported, but must not automatically reopen installation. Alternatively, if no durable one-use enforcement is provided, change the proposed authorization and documentation to accurately disclose that the feature-enabled pilot build retains install/remove capability with a **fresh, explicit in-app confirmation for every operation** and requires a separate post-pilot disable/cleanup control; obtain user's affirmative permission for that *broader* scope. Do not call the latter 'one-time'.

Never rely solely on the compile-time `Some` gate or a one-use preview nonce as proof of a one-use *overall pilot authorization*. First real jq copy remains NO-GO pending B3 exact-SHA review and user permission. Keep all standard builds incapable of real installation, Windows-only jq-pilot feature, isolated data directory, original file hash, no jq execution, PATH, network, agent config, Windows Hello, SAC/Defender changes or main edits. Issue #14 remains open.
