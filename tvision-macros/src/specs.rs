use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

/// Forwarders for `trait_name`, embedding field `field`, with crate path
/// `krate`. Returns `None` for an unknown trait. Caller filters provided/skipped.
pub fn forwarders(
    trait_name: &str,
    field: &Ident,
    krate: &TokenStream,
) -> Option<Vec<(&'static str, TokenStream)>> {
    match trait_name {
        "View" => Some(view(field, krate)),
        // ListViewer / Validator / menu traits: add an arm when a
        // field-delegating consumer exists (see docs/design/delegation-macros.md).
        _ => None,
    }
}

// MAINTENANCE: must list every method of `trait View` (src/view/view.rs). A
// missing defaulted method silently leaves `#[delegate]` sites on the trait
// default instead of forwarding. See the maintenance note in view.rs.
#[rustfmt::skip]
fn view(f: &Ident, k: &TokenStream) -> Vec<(&'static str, TokenStream)> {
    vec![
        ("state",
         quote! { fn state(&self) -> & #k::ViewState { self.#f.state() } }),
        ("state_mut",
         quote! { fn state_mut(&mut self) -> &mut #k::ViewState { self.#f.state_mut() } }),
        ("draw",
         quote! { fn draw(&mut self, ctx: &mut #k::DrawCtx) { self.#f.draw(ctx) } }),
        ("handle_event",
         quote! { fn handle_event(&mut self, ev: &mut #k::Event, ctx: &mut #k::Context) { self.#f.handle_event(ev, ctx) } }),
        ("set_state",
         quote! { fn set_state(&mut self, flag: #k::StateFlag, enable: bool, ctx: &mut #k::Context) { self.#f.set_state(flag, enable, ctx) } }),
        ("valid",
         quote! { fn valid(&mut self, cmd: #k::Command, ctx: &mut #k::Context) -> bool { self.#f.valid(cmd, ctx) } }),
        ("set_modal_answer",
         quote! { fn set_modal_answer(&mut self, cmd: #k::Command) { self.#f.set_modal_answer(cmd) } }),
        ("value",
         quote! { fn value(&self) -> ::core::option::Option<#k::FieldValue> { self.#f.value() } }),
        ("set_value",
         quote! { fn set_value(&mut self, v: #k::FieldValue) { self.#f.set_value(v) } }),
        ("set_value_ctx",
         quote! { fn set_value_ctx(&mut self, v: #k::FieldValue, ctx: &mut #k::Context) { self.#f.set_value_ctx(v, ctx) } }),
        ("awaken",
         quote! { fn awaken(&mut self) { self.#f.awaken() } }),
        ("size_limits",
         quote! { fn size_limits(&self, owner_size: #k::Point) -> (#k::Point, #k::Point) { self.#f.size_limits(owner_size) } }),
        ("calc_bounds",
         quote! { fn calc_bounds(&mut self, owner_size: #k::Point, delta: #k::Point) -> #k::Rect { self.#f.calc_bounds(owner_size, delta) } }),
        ("change_bounds",
         quote! { fn change_bounds(&mut self, bounds: #k::Rect) { self.#f.change_bounds(bounds) } }),
        ("on_bounds_changed",
         quote! { fn on_bounds_changed(&mut self, ctx: &mut #k::Context) { self.#f.on_bounds_changed(ctx) } }),
        ("cursor_request",
         quote! { fn cursor_request(&self) -> ::core::option::Option<#k::Point> { self.#f.cursor_request() } }),
        ("find_mut",
         quote! { fn find_mut(&mut self, id: #k::ViewId) -> ::core::option::Option<&mut dyn #k::View> { self.#f.find_mut(id) } }),
        ("remove_descendant",
         quote! { fn remove_descendant(&mut self, id: #k::ViewId, ctx: &mut #k::Context) -> bool { self.#f.remove_descendant(id, ctx) } }),
        ("focus_descendant",
         quote! { fn focus_descendant(&mut self, id: #k::ViewId, ctx: &mut #k::Context) -> bool { self.#f.focus_descendant(id, ctx) } }),
        ("reset_current",
         quote! { fn reset_current(&mut self, ctx: &mut #k::Context) { self.#f.reset_current(ctx) } }),
        ("settle_currency",
         quote! { fn settle_currency(&mut self, ctx: &mut #k::Context) { self.#f.settle_currency(ctx) } }),
        ("set_visible_descendant",
         quote! { fn set_visible_descendant(&mut self, id: #k::ViewId, visible: bool, ctx: &mut #k::Context) -> bool { self.#f.set_visible_descendant(id, visible, ctx) } }),
        ("number",
         quote! { fn number(&self) -> ::core::option::Option<i16> { self.#f.number() } }),
        ("grabs_focus_on_click",
         quote! { fn grabs_focus_on_click(&self) -> bool { self.#f.grabs_focus_on_click() } }),
        ("select_window_num",
         quote! { fn select_window_num(&mut self, num: i16, ctx: &mut #k::Context) -> bool { self.#f.select_window_num(num, ctx) } }),
        ("tile",
         quote! { fn tile(&mut self, r: #k::Rect) { self.#f.tile(r) } }),
        ("cascade",
         quote! { fn cascade(&mut self, r: #k::Rect) { self.#f.cascade(r) } }),
        ("apply_list_scroll",
         quote! { fn apply_list_scroll(&mut self, h: ::core::option::Option<i32>, v: ::core::option::Option<i32>, ctx: &mut #k::Context) { self.#f.apply_list_scroll(h, v, ctx) } }),
        ("update_menu_commands",
         quote! { fn update_menu_commands(&mut self, cs: &#k::CommandSet) { self.#f.update_menu_commands(cs) } }),
        ("set_menu_current",
         quote! { fn set_menu_current(&mut self, current: ::core::option::Option<usize>) { self.#f.set_menu_current(current) } }),
        ("get_help_ctx",
         quote! { fn get_help_ctx(&self) -> #k::HelpCtx { self.#f.get_help_ctx() } }),
        ("as_any_mut",
         quote! { fn as_any_mut(&mut self) -> ::core::option::Option<&mut dyn ::core::any::Any> { self.#f.as_any_mut() } }),
        ("descendant_global_bounds",
         quote! { fn descendant_global_bounds(&self, id: #k::ViewId, acc: #k::Point) -> ::core::option::Option<#k::Rect> { self.#f.descendant_global_bounds(id, acc) } }),
    ]
}
