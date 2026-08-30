/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use base64::Engine as _;
use cssparser::{Parser, ParserInput};
use dom_struct::dom_struct;
use html5ever::{LocalName, Prefix, local_name, ns};
use js::context::JSContext;
use js::rust::HandleObject;
use layout_api::SVGElementData;
use script_bindings::cell::DomRefCell;
use servo_url::ServoUrl;
use style::attr::AttrValue;
use style::color::AbsoluteColor;
use style::parser::ParserContext;
use style::stylesheets::Origin;
use style::values::specified::LengthPercentage;
use style_traits::{ParsingMode, ToCss};
use uuid::Uuid;
use xml5ever::serialize::TraversalScope;

use crate::dom::bindings::codegen::Bindings::DocumentBinding::DocumentMethods;
use crate::dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::root::{DomRoot, LayoutDom};
use crate::dom::bindings::str::DOMString;
use crate::dom::document::Document;
use crate::dom::element::attributes::storage::AttrRef;
use crate::dom::element::{AttributeMutation, Element};
use crate::dom::iterators::ShadowIncluding;
use crate::dom::node::virtualmethods::VirtualMethods;
use crate::dom::node::{
    ChildrenMutation, CloneChildrenFlag, Node, NodeDamage, NodeTraits, UnbindContext,
};
use crate::dom::svg::svggraphicselement::SVGGraphicsElement;

#[dom_struct]
pub(crate) struct SVGSVGElement {
    svggraphicselement: SVGGraphicsElement,
    uuid: String,
    // The XML source of subtree rooted at this SVG element, serialized into
    // a base64 encoded `data:` url. This is cached to avoid recomputation
    // on each layout and must be invalidated when the subtree changes.
    #[no_trace]
    cached_serialized_data_url: DomRefCell<Option<Result<ServoUrl, ()>>>,
    // The computed `color` value baked into `cached_serialized_data_url`'s markup (as a
    // `color` attribute on the root, for `currentColor` to resolve against -- see
    // `serialize_and_cache_subtree`), as of the last time it was (re)computed. Compared
    // against this element's current computed `color` on every layout pass (see
    // `SVGElementData::resolved_color`) so a `stroke`/`fill: currentColor` notices a restyle
    // this cached, non-cascade-aware serialization would otherwise never see.
    #[no_trace]
    cached_resolved_color: DomRefCell<Option<AbsoluteColor>>,
}

impl SVGSVGElement {
    fn new_inherited(
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
    ) -> SVGSVGElement {
        SVGSVGElement {
            svggraphicselement: SVGGraphicsElement::new_inherited(local_name, prefix, document),
            uuid: Uuid::new_v4().to_string(),
            cached_serialized_data_url: Default::default(),
            cached_resolved_color: Default::default(),
        }
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    pub(crate) fn new(
        cx: &mut js::context::JSContext,
        local_name: LocalName,
        prefix: Option<Prefix>,
        document: &Document,
        proto: Option<HandleObject>,
    ) -> DomRoot<SVGSVGElement> {
        Node::reflect_node_with_proto(
            cx,
            Box::new(SVGSVGElement::new_inherited(local_name, prefix, document)),
            document,
            proto,
        )
    }

    pub(crate) fn serialize_and_cache_subtree(
        &self,
        cx: &mut js::context::JSContext,
        resolved_color: AbsoluteColor,
    ) {
        let cloned_nodes = self.process_use_elements(cx);

        let serialize_result = self
            .upcast::<Node>()
            .xml_serialize(TraversalScope::IncludeNode);

        self.cleanup_cloned_nodes(cx, &cloned_nodes);

        let Ok(xml_source) = serialize_result else {
            *self.cached_serialized_data_url.borrow_mut() = Some(Err(()));
            *self.cached_resolved_color.borrow_mut() = None;
            return;
        };

        // Inline SVG content is handed off to `resvg`/`usvg` as a standalone document (see
        // this module's own doc comment) -- it has no notion of this page's own CSS cascade,
        // so a `stroke`/`fill: currentColor` in the source markup would otherwise always
        // resolve to the CSS-initial black. `usvg` does, however, correctly resolve
        // `currentColor` against a `color` presentation attribute on an ancestor within the
        // SVG document itself (real SVG/CSS-inheritance semantics, just scoped to that
        // document) -- so baking this element's own *actual* computed `color` onto the
        // serialized root as a `color="..."` attribute is enough to make `currentColor`
        // resolve correctly without needing real cross-document cascade-awareness.
        //
        // `resolved_color` is serialized via `into_srgb_legacy()` (forcing plain
        // `rgb(...)`/`rgba(...)` syntax), not a plain `to_css_string()` -- confirmed directly
        // (`usvg::parser::svgtree` logs "Failed to parse color value" for it) that `usvg`'s
        // CSS color parser doesn't understand modern CSS Color 4 functions like `oklch()`/
        // `oklab()`, which is exactly what `to_css_string()` produces for a color whose
        // *authored* CSS used those functions (as this app's own `oklch(...)` custom
        // properties do) -- `AbsoluteColor` preserves the color space it was specified in,
        // it doesn't normalize to sRGB on its own. A color usvg can't parse is silently
        // treated as unset, so `currentColor` fell back to its own default (black) even
        // though a `color` attribute was present in the markup.
        let xml_source: String = xml_source.into();
        let legacy_srgb_css = resolved_color.into_srgb_legacy().to_css_string();
        let xml_source = inject_root_color_attribute(&xml_source, &legacy_srgb_css);

        let base64_encoded_source = base64::engine::general_purpose::STANDARD.encode(xml_source);
        let data_url = format!("data:image/svg+xml;base64,{}", base64_encoded_source);
        match ServoUrl::parse(&data_url) {
            Ok(url) => {
                *self.cached_serialized_data_url.borrow_mut() = Some(Ok(url));
                *self.cached_resolved_color.borrow_mut() = Some(resolved_color);
            },
            Err(error) => error!("Unable to parse serialized SVG data url: {error}"),
        };
    }

    fn process_use_elements(&self, cx: &mut JSContext) -> Vec<DomRoot<Node>> {
        let mut cloned_nodes = Vec::new();
        let root_node = self.upcast::<Node>();

        for node in root_node.traverse_preorder(ShadowIncluding::No) {
            if let Some(element) = node.downcast::<Element>() &&
                element.local_name() == &local_name!("use") &&
                let Some(cloned) = self.process_single_use_element(cx, element)
            {
                cloned_nodes.push(cloned);
            }
        }

        cloned_nodes
    }

    fn process_single_use_element(
        &self,
        cx: &mut JSContext,
        use_element: &Element,
    ) -> Option<DomRoot<Node>> {
        let href = use_element.get_string_attribute(&local_name!("href"));
        let href_view = href.str();
        let id_str = href_view.strip_prefix("#")?;
        let id = DOMString::from(id_str);
        let document = self.upcast::<Node>().owner_doc();
        let referenced_element = document.GetElementById(cx, id)?;
        let referenced_node = referenced_element.upcast::<Node>();
        let has_svg_ancestor = referenced_node
            .inclusive_ancestors(ShadowIncluding::No)
            .any(|ancestor| ancestor.is::<SVGSVGElement>());
        if !has_svg_ancestor {
            return None;
        }
        let cloned_node = Node::clone(
            cx,
            referenced_node,
            None,
            CloneChildrenFlag::CloneChildren,
            None,
        );
        let root_node = self.upcast::<Node>();
        let _ = root_node.AppendChild(cx, &cloned_node);

        Some(cloned_node)
    }

    fn cleanup_cloned_nodes(&self, cx: &mut JSContext, cloned_nodes: &[DomRoot<Node>]) {
        if cloned_nodes.is_empty() {
            return;
        }
        let root_node = self.upcast::<Node>();

        for cloned_node in cloned_nodes {
            let _ = root_node.RemoveChild(cx, cloned_node);
        }
    }

    fn invalidate_cached_serialized_subtree_and_rasterization_result(&self) {
        let owner_window = self.owner_window();
        owner_window
            .image_cache()
            .evict_rasterized_image(&self.uuid);
        if let Some(Ok(url)) = &*self.cached_serialized_data_url.borrow() {
            owner_window.layout_mut().remove_cached_image(url);
            owner_window.image_cache().evict_completed_image(
                url,
                owner_window.origin().immutable(),
                &None,
            );
        }

        *self.cached_serialized_data_url.borrow_mut() = None;
        *self.cached_resolved_color.borrow_mut() = None;
        self.upcast::<Node>().dirty(NodeDamage::Other);
    }
}

/// Inserts a `color="<css_color>"` attribute into a serialized XML document's root opening
/// tag, so `currentColor` resolves against it inside that document -- see
/// `SVGSVGElement::serialize_and_cache_subtree`'s own doc comment for why. Scans for the end
/// of the opening tag (the first `>` outside of a quoted attribute value) rather than
/// assuming a fixed prefix, since the root element's attribute order/count varies.
///
/// `css_color` is trusted to be a plain CSS color-function serialization (from
/// `AbsoluteColor::to_css_string()` -- digits/letters/`,`/`%`/whitespace/parens only, never
/// `<`, `&`, or a quote character), so it's inserted as-is with no escaping.
fn inject_root_color_attribute(xml_source: &str, css_color: &str) -> String {
    let bytes = xml_source.as_bytes();
    let mut quote: Option<u8> = None;
    for (index, &byte) in bytes.iter().enumerate() {
        match quote {
            Some(quote_byte) if byte == quote_byte => quote = None,
            Some(_) => {},
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => {
                // A self-closing root (no children) ends in "/>" -- insert before the "/"
                // rather than between it and ">", which would otherwise produce
                // `<svg .../ color="...">` (a malformed, no-longer-self-closing tag).
                let insert_at = if index > 0 && bytes[index - 1] == b'/' {
                    index - 1
                } else {
                    index
                };
                let mut result = String::with_capacity(xml_source.len() + css_color.len() + 10);
                result.push_str(&xml_source[..insert_at]);
                result.push_str(" color=\"");
                result.push_str(css_color);
                result.push('"');
                result.push_str(&xml_source[insert_at..]);
                return result;
            },
            None => {},
        }
    }
    // No opening tag found -- shouldn't happen for a real serialized element, but return the
    // source unchanged rather than risk producing malformed XML.
    xml_source.to_owned()
}

impl<'dom> LayoutDom<'dom, SVGSVGElement> {
    #[expect(unsafe_code)]
    pub(crate) fn data(self) -> SVGElementData<'dom> {
        let svg_id = self.unsafe_get().uuid.clone();
        let element = self.upcast::<Element>();
        let width = element.get_attr_for_layout(&ns!(), &local_name!("width"));
        let height = element.get_attr_for_layout(&ns!(), &local_name!("height"));
        let view_box = element.get_attr_for_layout(&ns!(), &local_name!("viewBox"));
        SVGElementData {
            source: unsafe {
                self.unsafe_get()
                    .cached_serialized_data_url
                    .borrow_for_layout()
                    .clone()
            },
            resolved_color: unsafe {
                *self.unsafe_get().cached_resolved_color.borrow_for_layout()
            },
            width,
            height,
            view_box,
            svg_id,
        }
    }
}

impl VirtualMethods for SVGSVGElement {
    fn super_type(&self) -> Option<&dyn VirtualMethods> {
        Some(self.upcast::<SVGGraphicsElement>() as &dyn VirtualMethods)
    }

    fn attribute_mutated(
        &self,
        cx: &mut js::context::JSContext,
        attr: AttrRef<'_>,
        mutation: AttributeMutation,
    ) {
        self.super_type()
            .unwrap()
            .attribute_mutated(cx, attr, mutation);

        self.invalidate_cached_serialized_subtree_and_rasterization_result();
    }

    fn attribute_affects_presentational_hints(&self, attr: AttrRef<'_>) -> bool {
        match attr.local_name() {
            &local_name!("width") | &local_name!("height") => true,
            _ => self
                .super_type()
                .unwrap()
                .attribute_affects_presentational_hints(attr),
        }
    }

    fn parse_plain_attribute(&self, name: &LocalName, value: DOMString) -> AttrValue {
        match *name {
            local_name!("width") | local_name!("height") => {
                let value = &value.str();
                let parser_input = &mut ParserInput::new(value);
                let parser = &mut Parser::new(parser_input);
                let doc = self.owner_document();
                let url = doc.url().into_url().into();
                let context = ParserContext::new(
                    Origin::Author,
                    &url,
                    None,
                    ParsingMode::ALLOW_UNITLESS_LENGTH,
                    doc.quirks_mode(),
                    /* namespaces = */ Default::default(),
                    None,
                    None,
                    /* attr_taint = */ Default::default(),
                );
                let val = LengthPercentage::parse_quirky(
                    &context,
                    parser,
                    style::values::specified::AllowQuirks::Always,
                );
                AttrValue::LengthPercentage(value.to_string(), val.ok())
            },
            _ => self
                .super_type()
                .unwrap()
                .parse_plain_attribute(name, value),
        }
    }

    fn children_changed(&self, cx: &mut JSContext, mutation: &ChildrenMutation) {
        if let Some(super_type) = self.super_type() {
            super_type.children_changed(cx, mutation);
        }

        self.invalidate_cached_serialized_subtree_and_rasterization_result();
    }

    fn unbind_from_tree(&self, cx: &mut js::context::JSContext, context: &UnbindContext<'_>) {
        if let Some(s) = self.super_type() {
            s.unbind_from_tree(cx, context);
        }

        self.invalidate_cached_serialized_subtree_and_rasterization_result();
    }
}
