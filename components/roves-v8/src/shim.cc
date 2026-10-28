/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Public V8 150.4 APIs rusty_v8 150 does not bind, compiled against the pinned crate's own
// headers (see build.rs): the compiler derives every symbol and ABI from the declarations.
// Only V8-exported functions are called; handles cross as the single pointer a `Local` is
// (rusty_v8 represents `Local<T>` the same way).

#include <cstring>
#include <new>

#include "v8-callbacks.h"
#include "v8-context.h"
#include "v8-isolate.h"
#include "v8-local-handle.h"
#include "v8-object.h"
#include "v8-script.h"
#include "v8-template.h"

namespace {

template <typename T>
const void* local_to_ptr(v8::Local<T> local) {
  static_assert(sizeof(v8::Local<T>) == sizeof(void*), "a Local is one pointer");
  const void* ptr;
  std::memcpy(&ptr, &local, sizeof(ptr));
  return ptr;
}

template <typename T>
v8::Local<T> ptr_to_local(const void* ptr) {
  v8::Local<T> local;
  std::memcpy(&local, &ptr, sizeof(ptr));
  return local;
}

}  // namespace

extern "C" {

// Implemented in Rust (`codegen.rs`): decides on one string compilation. Returns whether it
// may proceed; `*modified` is then the source to compile instead (null: the original).
bool roves_v8_codegen_from_strings(const void* context, const void* source, bool is_code_like, const void** modified);

}  // extern "C"

namespace {

v8::ModifyCodeGenerationFromStringsResult codegen_from_strings(v8::Local<v8::Context> context,
                                                               v8::Local<v8::Value> source,
                                                               bool is_code_like) {
  const void* modified = nullptr;
  v8::ModifyCodeGenerationFromStringsResult result;
  result.codegen_allowed =
      roves_v8_codegen_from_strings(local_to_ptr(context), local_to_ptr(source), is_code_like, &modified);
  if (result.codegen_allowed && modified != nullptr) {
    result.modified_source = ptr_to_local<v8::String>(modified);
  }
  return result;
}

}  // namespace

extern "C" {

// `Isolate::SetModifyCodeGenerationFromStringsCallback(ModifyCodeGenerationFromStringsCallback2)`.
void roves_v8_set_codegen_callback(v8::Isolate* isolate) {
  isolate->SetModifyCodeGenerationFromStringsCallback(codegen_from_strings);
}

// `Isolate::GetIncumbentContext()`, as its handle's pointer.
const void* roves_v8_incumbent_context(v8::Isolate* isolate) {
  return local_to_ptr(isolate->GetIncumbentContext());
}

// `Context::BackupIncumbentScope`, constructed in storage the caller owns (V8 links scopes by
// address, so the storage must not move until the scope is destroyed).
size_t roves_v8_backup_incumbent_scope_size() {
  return sizeof(v8::Context::BackupIncumbentScope);
}

size_t roves_v8_backup_incumbent_scope_align() {
  return alignof(v8::Context::BackupIncumbentScope);
}

void roves_v8_backup_incumbent_scope_enter(void* storage, const void* context) {
  new (storage) v8::Context::BackupIncumbentScope(ptr_to_local<v8::Context>(context));
}

void roves_v8_backup_incumbent_scope_exit(void* storage) {
  static_cast<v8::Context::BackupIncumbentScope*>(storage)->~BackupIncumbentScope();
}

}  // extern "C"

// Additional public APIs needed by the mozjs adapter on rusty_v8 150.
extern "C" int roves_v8_script_id(const void* script) {
  return ptr_to_local<v8::UnboundScript>(script)->GetId();
}
extern "C" bool roves_v8_is_callable(const void* object) {
  return ptr_to_local<v8::Object>(object)->IsCallable();
}
extern "C" bool roves_v8_is_constructor(const void* object) {
  return ptr_to_local<v8::Object>(object)->IsConstructor();
}
extern "C" void roves_v8_set_call_handler(const void* object_template,
                                         v8::FunctionCallback callback,
                                         const void* data) {
  ptr_to_local<v8::ObjectTemplate>(object_template)->SetCallAsFunctionHandler(
      callback, ptr_to_local<v8::Value>(data));
}
