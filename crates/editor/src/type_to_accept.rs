use crate::{Anchor, Editor};
use gpui::{Context, Window};

/// An active type-to-accept session attached to an editor.
///
/// When the AI agent proposes a change, instead of accepting it immediately
/// the user must type each character of the new text. The buffer already
/// contains the agent's text; the session tracks the user's progress through
/// that text and gates the final acceptance on full completion.
pub struct TypeToAcceptSession {
    pub(crate) target_text: String,
    pub(crate) bytes_typed: usize,
    pub(crate) error_count: usize,
    pub(crate) start_anchor: Anchor,
    pub(crate) end_anchor: Anchor,
    pub(crate) on_complete: Option<Box<dyn FnOnce(&mut Window, &mut Context<Editor>)>>,
}

impl TypeToAcceptSession {
    pub fn new(
        target_text: String,
        start_anchor: Anchor,
        end_anchor: Anchor,
        on_complete: Box<dyn FnOnce(&mut Window, &mut Context<Editor>)>,
    ) -> Self {
        Self {
            target_text,
            bytes_typed: 0,
            error_count: 0,
            start_anchor,
            end_anchor,
            on_complete: Some(on_complete),
        }
    }

    pub fn next_expected_char(&self) -> Option<char> {
        self.target_text[self.bytes_typed..].chars().next()
    }

    pub fn is_complete(&self) -> bool {
        self.bytes_typed >= self.target_text.len()
    }

    pub fn progress(&self) -> f32 {
        if self.target_text.is_empty() {
            1.0
        } else {
            self.bytes_typed as f32 / self.target_text.len() as f32
        }
    }

    pub fn total_chars(&self) -> usize {
        self.target_text.chars().count()
    }

    pub fn typed_chars(&self) -> usize {
        self.target_text[..self.bytes_typed].chars().count()
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::build_editor;
    use gpui::{AppContext, TestAppContext};
    use multi_buffer::{MultiBuffer, MultiBufferOffset};
    use settings::SettingsStore;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assets::Assets.load_test_fonts(cx);
            let store = SettingsStore::test(cx);
            cx.set_global(store);
            theme::init(theme::LoadThemes::JustBase, cx);
            release_channel::init(semver::Version::new(0, 0, 0), cx);
            crate::init(cx);
        });
    }

    #[test]
    fn test_session_next_expected_char() {
        let session = TypeToAcceptSession {
            target_text: "hello".to_string(),
            bytes_typed: 0,
            error_count: 0,
            start_anchor: Anchor::min(),
            end_anchor: Anchor::max(),
            on_complete: None,
        };

        assert_eq!(session.next_expected_char(), Some('h'));
        assert!(!session.is_complete());
        assert_eq!(session.progress(), 0.0);
        assert_eq!(session.total_chars(), 5);
        assert_eq!(session.typed_chars(), 0);
    }

    #[test]
    fn test_session_progress_tracking() {
        let mut session = TypeToAcceptSession {
            target_text: "hi".to_string(),
            bytes_typed: 0,
            error_count: 0,
            start_anchor: Anchor::min(),
            end_anchor: Anchor::max(),
            on_complete: None,
        };

        assert_eq!(session.next_expected_char(), Some('h'));
        session.bytes_typed += 'h'.len_utf8();
        assert_eq!(session.next_expected_char(), Some('i'));
        assert_eq!(session.typed_chars(), 1);
        assert!(!session.is_complete());

        session.bytes_typed += 'i'.len_utf8();
        assert_eq!(session.next_expected_char(), None);
        assert!(session.is_complete());
        assert_eq!(session.progress(), 1.0);
        assert_eq!(session.typed_chars(), 2);
    }

    #[test]
    fn test_session_empty_text() {
        let session = TypeToAcceptSession {
            target_text: String::new(),
            bytes_typed: 0,
            error_count: 0,
            start_anchor: Anchor::min(),
            end_anchor: Anchor::max(),
            on_complete: None,
        };

        assert!(session.is_complete());
        assert_eq!(session.progress(), 1.0);
        assert_eq!(session.next_expected_char(), None);
    }

    #[test]
    fn test_session_unicode() {
        let mut session = TypeToAcceptSession {
            target_text: "café".to_string(),
            bytes_typed: 0,
            error_count: 0,
            start_anchor: Anchor::min(),
            end_anchor: Anchor::max(),
            on_complete: None,
        };

        assert_eq!(session.total_chars(), 4);

        session.bytes_typed += 'c'.len_utf8();
        session.bytes_typed += 'a'.len_utf8();
        session.bytes_typed += 'f'.len_utf8();
        assert_eq!(session.next_expected_char(), Some('é'));
        assert_eq!(session.typed_chars(), 3);
        assert!(!session.is_complete());

        session.bytes_typed += 'é'.len_utf8();
        assert!(session.is_complete());
    }

    #[test]
    fn test_session_error_count() {
        let mut session = TypeToAcceptSession {
            target_text: "ab".to_string(),
            bytes_typed: 0,
            error_count: 0,
            start_anchor: Anchor::min(),
            end_anchor: Anchor::max(),
            on_complete: None,
        };

        session.error_count += 1;
        session.error_count += 1;
        assert_eq!(session.error_count(), 2);
        assert_eq!(session.bytes_typed, 0);
    }

    #[gpui::test]
    fn test_type_to_accept_basic_flow(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("hello world", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        let completed = Rc::new(RefCell::new(false));

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(5));

            let completed = completed.clone();
            editor.start_type_to_accept(
                "hello".to_string(),
                start,
                end,
                Box::new(move |_window, _cx| {
                    *completed.borrow_mut() = true;
                }),
                window,
                cx,
            );

            assert!(!editor.type_to_accept_sessions.is_empty());
        });

        _ = editor.update(cx, |editor, window, cx| {
            assert!(editor.handle_type_to_accept_char("h", window, cx));
            assert!(editor.handle_type_to_accept_char("e", window, cx));
            assert!(editor.handle_type_to_accept_char("l", window, cx));
            assert!(editor.handle_type_to_accept_char("l", window, cx));
            assert!(!editor.type_to_accept_sessions.is_empty());

            assert!(editor.handle_type_to_accept_char("o", window, cx));
            assert!(editor.type_to_accept_sessions.is_empty());
        });

        assert!(*completed.borrow());
    }

    #[gpui::test]
    fn test_type_to_accept_wrong_character(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("abc", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(3));

            editor.start_type_to_accept(
                "abc".to_string(),
                start,
                end,
                Box::new(|_window, _cx| {}),
                window,
                cx,
            );

            // Wrong character should be consumed but not advance
            assert!(editor.handle_type_to_accept_char("x", window, cx));
            let session = editor.type_to_accept_sessions.first().unwrap();
            assert_eq!(session.error_count, 1);
            assert_eq!(session.bytes_typed, 0);

            // Correct character should advance
            assert!(editor.handle_type_to_accept_char("a", window, cx));
            let session = editor.type_to_accept_sessions.first().unwrap();
            assert_eq!(session.bytes_typed, 1);
        });
    }

    #[gpui::test]
    fn test_type_to_accept_backspace(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("abc", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(3));

            editor.start_type_to_accept(
                "abc".to_string(),
                start,
                end,
                Box::new(|_window, _cx| {}),
                window,
                cx,
            );

            // Type 'a'
            editor.handle_type_to_accept_char("a", window, cx);
            assert_eq!(editor.type_to_accept_sessions.first().unwrap().bytes_typed, 1);

            // Backspace should go back
            assert!(editor.handle_type_to_accept_backspace(window, cx));
            assert_eq!(editor.type_to_accept_sessions.first().unwrap().bytes_typed, 0);

            // Backspace at beginning should be consumed but do nothing
            assert!(editor.handle_type_to_accept_backspace(window, cx));
            assert_eq!(editor.type_to_accept_sessions.first().unwrap().bytes_typed, 0);
        });
    }

    #[gpui::test]
    fn test_type_to_accept_skip(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("hello", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let completed = Rc::new(RefCell::new(false));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(5));

            let completed = completed.clone();
            editor.start_type_to_accept(
                "hello".to_string(),
                start,
                end,
                Box::new(move |_window, _cx| {
                    *completed.borrow_mut() = true;
                }),
                window,
                cx,
            );

            editor.handle_type_to_accept_char("h", window, cx);
            editor.handle_type_to_accept_char("e", window, cx);

            editor.skip_type_to_accept(&crate::SkipTypeToAccept, window, cx);
            assert!(editor.type_to_accept_sessions.is_empty());
        });

        assert!(*completed.borrow());
    }

    #[gpui::test]
    fn test_type_to_accept_empty_text_completes_immediately(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let completed = Rc::new(RefCell::new(false));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(0));

            let completed = completed.clone();
            editor.start_type_to_accept(
                String::new(),
                start,
                end,
                Box::new(move |_window, _cx| {
                    *completed.borrow_mut() = true;
                }),
                window,
                cx,
            );

            assert!(editor.type_to_accept_sessions.is_empty());
        });

        assert!(*completed.borrow());
    }

    #[gpui::test]
    fn test_type_to_accept_no_session_passthrough(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("hello", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            assert!(!editor.handle_type_to_accept_char("h", window, cx));
            assert!(!editor.handle_type_to_accept_backspace(window, cx));
        });
    }

    #[gpui::test]
    fn test_type_to_accept_multichar_passthrough(cx: &mut TestAppContext) {
        init_test(cx);

        let buffer = cx.update(|cx| cx.new(|cx| language::Buffer::local("hello", cx)));
        let multi_buffer = cx.update(|cx| cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx)));

        let editor = cx.add_window(|window, cx| {
            build_editor(multi_buffer.clone(), window, cx)
        });

        _ = editor.update(cx, |editor, window, cx| {
            let snapshot = editor.buffer.read(cx).snapshot(cx);
            let start = snapshot.anchor_after(MultiBufferOffset(0));
            let end = snapshot.anchor_before(MultiBufferOffset(5));

            editor.start_type_to_accept(
                "hello".to_string(),
                start,
                end,
                Box::new(|_window, _cx| {}),
                window,
                cx,
            );

            // Multi-character input (paste) should pass through
            assert!(!editor.handle_type_to_accept_char("he", window, cx));
        });
    }
}
