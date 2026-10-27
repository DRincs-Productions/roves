# 0375 — keep indexed DOM keys separate from numeric named keys

The Form supported-name list can contain "0" while ownKeys already appends supported
indices. Native IdVector appends without deduplication and the proxy bridge forwards it
unchanged. The new native proof confirms duplicate integer ids reach V8 as duplicate
string keys and Reflect.ownKeys throws TypeError; valid keys still work before and after.
Do not hide invalid native handlers with generic proxy deduplication.

Reuse utils::get_array_index_from_id only for interfaces with indexed getters. Omit
canonical array-index names from their named-key pass, before prototype callbacks and in
the final override path. This follows the binding's existing indexed-first/ignoreNamedProps
read, GOPD and hasOwn rule, including out-of-range indices. The WebIDL ownKeys text does not
explicitly spell out this filter: its extension to out-of-range named keys is an integration
choice for consistency with those existing read hooks, not a new full-conformance claim.
Named-only Document retains numeric names. "00", "-1" and "4294967295" remain named keys.
No new field, dependency, numeric parser, HashSet, ordering scheme or persistent state.

## Validation

- Native proxy contracts (filter proxies::, includes WindowProxy):18 normal/18JITless pass.
  The duplicate-key proof uses the real native ProxyTraps/IdVector path, not a JS-only proxy.
- Runner42 pass (8 POSIX skips), Rust helper syntax valid. Source analysis shows the old
  DOM loop can produce the overlapping ids; no local full-browser negative was run.
- Required browser smoke covers Form readonly indices, overlapping and out-of-range numeric
  names, noncanonical numeric names, no numeric-name prototype callbacks and unchanged
  numeric named properties on Document. Normal/JITless browser confirmation remains CI-only.
- Logs and prompts: target/task-0375. Chrome MCP unavailable (Target closed), so no fresh
  browser reference claimed. No CPU/RAM saving claimed.

The0372 own-property short circuit/reentrancy filtering and0371 exact/unpinned name
conversion are preserved. SourceText0361, define/delete visibility and overall page
retention remain open. This is not a universal repair of arbitrary malformed native traps.
