//use syn::Type;
use makepad_render::*;
use makepad_widget::*;

use crate::jseditor::*;
use crate::rusteditor::*;
use crate::plaineditor::*;
use crate::searchindex::*;

#[derive(Clone)]
pub struct FileEditorTemplates {
    pub rust_editor: RustEditor,
    pub js_editor: JSEditor,
    pub plain_editor: PlainEditor
    //text_editor: TextEditor
}

#[derive(Clone)]
pub enum FileEditor {
    Rust(RustEditor),
    JS(JSEditor),
    Plain(PlainEditor)
    //Text(TextEditor)
}

impl FileEditor {
    pub fn handle_file_editor(&mut self, cx: &mut Cx, event: &mut Event, text_buffer: &mut TextBuffer) -> TextEditorEvent {
        match self {
            FileEditor::Rust(re) => re.handle_rust_editor(cx, event, text_buffer),
            FileEditor::JS(re) => re.handle_js_editor(cx, event, text_buffer),
            FileEditor::Plain(re) => re.handle_plain_editor(cx, event, text_buffer),
        }
    }
    
    pub fn set_key_focus(&mut self, cx: &mut Cx) {
        match self {
            FileEditor::Rust(re) => re.text_editor.set_key_focus(cx),
            FileEditor::JS(re) => re.text_editor.set_key_focus(cx),
            FileEditor::Plain(re) => re.text_editor.set_key_focus(cx),
        }
    }

    pub fn get_scroll_pos(&mut self, cx: &mut Cx)->Vec2{
        match self {
            FileEditor::Rust(re) => re.text_editor.view.get_scroll_pos(cx),
            FileEditor::JS(re) => re.text_editor.view.get_scroll_pos(cx),
            FileEditor::Plain(re) => re.text_editor.view.get_scroll_pos(cx),
        }
    }
    
    pub fn get_ident_around_last_cursor(&mut self, text_buffer: &mut TextBuffer)->String{
        match self {
            FileEditor::Rust(re) => re.text_editor.cursors.get_ident_around_last_cursor(text_buffer),
            FileEditor::JS(re) => re.text_editor.cursors.get_ident_around_last_cursor(text_buffer),
            FileEditor::Plain(re) => re.text_editor.cursors.get_ident_around_last_cursor(text_buffer),
        }
    }
    
    pub fn set_scroll_pos_on_load(&mut self, pos:Vec2){
        match self {
            FileEditor::Rust(re) => re.text_editor._scroll_pos_on_load = Some(pos),
            FileEditor::JS(re) => re.text_editor._scroll_pos_on_load = Some(pos),
            FileEditor::Plain(re) => re.text_editor._scroll_pos_on_load = Some(pos),
        }
    }

    pub fn draw_file_editor(&mut self, cx: &mut Cx, text_buffer: &mut TextBuffer, search_index: &mut SearchIndex) {
        match self {
            FileEditor::Rust(re) => re.draw_rust_editor(cx, text_buffer, search_index),
            FileEditor::JS(re) => re.draw_js_editor(cx, text_buffer, search_index),
            FileEditor::Plain(re) => re.draw_plain_editor(cx, text_buffer, search_index),
        }
    }
    
    pub fn update_token_chunks(path:&str, text_buffer: &mut TextBuffer, search_index: &mut SearchIndex){
        // check which file extension we have to spawn a new editor
        if path.ends_with(".rs") || path.ends_with(".toml")  || path.ends_with(".ron"){
            RustTokenizer::update_token_chunks(text_buffer, search_index);
        }
        else if path.ends_with(".js") || path.ends_with(".html"){
            JSTokenizer::update_token_chunks(text_buffer, search_index);
        }
        else {
            PlainTokenizer::update_token_chunks(text_buffer, search_index);
        }
    }
    
    pub fn create_file_editor_for_path(path: &str, template: &FileEditorTemplates) -> FileEditor {
        // check which file extension we have to spawn a new editor
        if path.ends_with(".rs") || path.ends_with(".toml")  || path.ends_with(".ron"){
            FileEditor::Rust(RustEditor {
                ..template.rust_editor.clone()
            })
        }
        else if path.ends_with(".js") || path.ends_with(".html"){
            FileEditor::JS(JSEditor {
                ..template.js_editor.clone()
            })
        }
        else {
            FileEditor::Plain(PlainEditor {
                ..template.plain_editor.clone()
            })
        }
    }
}

pub fn path_file_name(path: &str) -> String {
    if let Some(pos) = path.rfind('/') {
        path[pos + 1..path.len()].to_string()
    }
    else {
        path.to_string()
    }
}
