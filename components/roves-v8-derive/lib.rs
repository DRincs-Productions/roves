/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! `#[derive(Trace)]` for `roves_v8::Trace`, the V8 (cppgc unified heap) counterpart of Servo's
//! `#[derive(JSTraceable)]`. It accepts the same field attributes, so a `#[dom_struct]` can derive
//! both while the DOM moves from SpiderMonkey to V8:
//!
//! - `#[no_trace]` skips a field that holds no traced reference (with a reason,
//!   `#[no_trace = "..."]`, as in `JSTraceable`; the reason is not checked here);
//! - `#[custom_trace]` traces a field through `roves_v8::CustomTrace`, for foreign types that
//!   cannot implement `Trace` because of the orphan rule.
//!
//! Every other field is traced through its own `Trace` impl, and every type parameter gets a
//! `Trace` bound.

use syn::parse_quote;
use synstructure::{decl_derive, quote};

decl_derive!([Trace, attributes(no_trace, custom_trace)] =>
/// Implements `roves_v8::Trace` by tracing every field (see the crate documentation).
trace_derive);

fn trace_derive(s: synstructure::Structure) -> proc_macro2::TokenStream {
    let match_body = s.each(|binding| {
        for attr in binding.ast().attrs.iter() {
            if attr.path().is_ident("no_trace") {
                return None;
            }
            if attr.path().is_ident("custom_trace") {
                return Some(quote!(::roves_v8::CustomTrace::trace(#binding, tracer);));
            }
        }
        Some(quote!(::roves_v8::Trace::trace(#binding, tracer);))
    });

    let ast = s.ast();
    let name = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let mut where_clause = where_clause.cloned().unwrap_or_else(|| parse_quote!(where));
    for param in ast.generics.type_params() {
        let ident = &param.ident;
        where_clause.predicates.push(parse_quote!(#ident: ::roves_v8::Trace));
    }

    quote! {
        impl #impl_generics ::roves_v8::Trace for #name #ty_generics #where_clause {
            #[inline]
            #[allow(unused_variables)]
            fn trace(&self, tracer: &mut ::roves_v8::Tracer) {
                match *self {
                    #match_body
                }
            }
        }
    }
}
