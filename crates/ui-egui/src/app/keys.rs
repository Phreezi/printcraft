use super::*;

impl PdfCraftApp {
    pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
        use egui::Key;
        // The first-run language prompt only takes a choice.
        if self.language_prompt {
            return;
        }
        // "Save changes?" is modal: its keys are its own (⌘D is Don't save there, not Document
        // properties; Escape cancels it rather than clearing a selection), and nothing may run
        // underneath it. They are read here, before the canvas can consume them.
        if self.close_request.is_some() {
            // These dialogs are drawn after the save prompt and own the keyboard while open.
            // Don't let Escape or Return resolve unsaved work underneath one.
            let covered = self.pending_link.is_some()
                || self.pending_remote_link.is_some()
                || self.pending_full_screen.is_some()
                || self.updates.open
                || self.dialog.is_some()
                || self.unlock_prompt.is_some();
            if !covered
                && self.draws_overlays()
                && let Some(choice) = dialogs::save_prompt_key(ctx, &mut self.save_prompt_button)
            {
                self.resolve_close(ctx, choice);
            }
            return;
        }
        // The other dialogs and prompts are modal too (⌘W closed the file behind Preferences,
        // #870).
        if self.modal_open() {
            return;
        }
        if let Some(view) = self.active.and_then(|i| self.views.get_mut(i))
            && view.auto_scroll.escape(ctx)
        {
            return;
        }
        // The in-window menu bar holds the keys while one of its menus is open or it has the
        // keyboard (Alt, F10); a command shortcut leaves it and runs.
        craft_menu::bar::leave_for_shortcut(ctx);
        if !craft_menu::bar::is_open(ctx) && !craft_menu::bar::is_focused(ctx) {
            self.registry_shortcuts(ctx);
        }
        // Before the canvas's own keys: ⌘A there selects all text.
        self.edit_text_keys(ctx);
        // (Esc closes a Combine file shown large first.)
        if self.full_screen && self.combine_tab.preview.is_none() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.set_full_screen(ctx, false);
        }
        if let Some(i) = self.active {
            // Select all belongs to the document or page grid, unless a text field or
            // overlay owns the keyboard. Other canvas shortcuts keep their own handling.
            let pages_owns_keyboard =
                self.right == Some(RightPanel::Pages) && self.views[i].pages_focus_id.is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
            if self.dialog.is_none()
                && !self.palette_open
                && (pages_owns_keyboard || !ctx.egui_wants_keyboard_input())
                && ctx.input_mut(|input| input.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, Key::A)))
            {
                if pages_owns_keyboard {
                    self.views[i].select_all_pages();
                } else if self.is_preparing() {
                    if let Some(doc) = self.session.get(self.views[i].id) {
                        prepare::select_all(&mut self.views[i], &doc.form);
                    }
                } else {
                    self.views[i].select_all();
                }
            }
            self.tool_keys(i, ctx);
            if self.quick_tool == QuickTool::EditText
                && let Some(doc) = self.session.get(self.views[i].id).filter(|d| d.allows_modification())
            {
                object_ui::nudge_keys(ctx, &mut self.views[i], doc);
                object_ui::delete_keys(ctx, &mut self.views[i]);
            } else if let Some(doc) = self.session.get(self.views[i].id).filter(|d| d.allows_annotation()) {
                comments::nudge_keys(ctx, &mut self.views[i], &doc.info);
            }
            // A selected field moves with the arrows too (Prepare a form, #2608), before the view
            // turns the page with them.
            if self.is_preparing()
                && let Some(doc) = self.session.get(self.views[i].id).filter(|d| d.allows_modification())
            {
                prepare::nudge_keys(ctx, &mut self.views[i], doc);
            }
            let space = self.space_pages(ctx);
            canvas::shortcuts(&mut self.views[i], ctx, self.scroll_speed, space);
            self.clipboard_keys(i, ctx);
            self.image_pick_keys(i, ctx);
        }
    }
}

impl PdfCraftApp {
    /// The quick tools' keys, as in Acrobat: V selects, H pans, and holding Space pans until it
    /// is released. Plain letters, so not while a text field, a form field, a dialog or the
    /// palette has the keyboard.
    fn tool_keys(&mut self, i: usize, ctx: &egui::Context) {
        use egui::Key;
        let free = self.dialog.is_none() && !self.palette_open && !ctx.egui_wants_keyboard_input() && self.views[i].forms.focus.is_none();
        let (plain, space) = ctx.input(|input| (input.modifiers.is_none(), input.key_down(Key::Space)));
        if let Some(previous) = self.space_hand
            && !space
        {
            self.space_hand = None;
            if self.quick_tool == QuickTool::Hand {
                self.quick_tool = previous;
            }
        }
        if !free || !plain {
            return;
        }
        if ctx.input(|input| input.key_pressed(Key::V)) {
            self.quick_tool = QuickTool::Select;
            self.space_hand = None;
        }
        if ctx.input(|input| input.key_pressed(Key::H)) {
            self.quick_tool = QuickTool::Hand;
            self.space_hand = None;
        }
        // Holding Space pans, except where it pages through the document (PeDeeFe, as Acrobat's
        // Space: `space_pages`).
        if space && self.space_hand.is_none() && self.quick_tool != QuickTool::Hand && !self.space_pages(ctx) {
            self.space_hand = Some(self.quick_tool);
            self.quick_tool = QuickTool::Hand;
        }
    }

    /// Whether Space pages through the active document now: nothing else has the keyboard (a
    /// dialog, the palette, a menu, a text field or a focused button, which Space would press),
    /// and no mode types or edits with it (Edit a PDF, Add text, Edit text, Prepare a form, the
    /// page grid).
    pub(crate) fn space_pages(&self, ctx: &egui::Context) -> bool {
        let organizing = self.active.and_then(|i| self.views.get(i)).is_none_or(|v| v.organize);
        let editing = (self.left_open && self.left == LeftPanel::Tool("edit"))
            || matches!(self.quick_tool, QuickTool::AddText | QuickTool::EditText)
            || self.is_preparing();
        // Space held for the Hand (where it doesn't page) keeps panning, not paging.
        self.space_hand.is_none()
            && self.dialog.is_none()
            && !self.palette_open
            && !self.modal_open()
            && !organizing
            && !editing
            && !egui::Popup::is_any_open(ctx)
            && !ctx.egui_wants_keyboard_input()
            && ctx.memory(|m| m.focused().is_none())
    }
}
