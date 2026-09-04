//! The composer, the list, and one row.

use dioxus::prelude::*;
use todo_core::{Direct, Todo};

use crate::toast::{Kind, Toasts};
use crate::{dispatch, log, Ui};

/// The input that creates todos for one user.
#[component]
pub fn Composer(user_id: String) -> Element {
    let ui = use_context::<Ui>();
    let toasts = use_context::<Toasts>();
    let mut draft = use_signal(String::new);

    // A `Callback` rather than a closure because two handlers need it. A `FnMut` closure can only
    // be moved into one of them; `Callback` is `Copy`, which is the whole reason the hook exists.
    let submit = use_callback(move |()| {
        let title = draft.read().trim().to_owned();
        if title.is_empty() {
            return;
        }
        // Cleared before the call rather than after it. `create` projects the row locally and only
        // then enqueues, so by the time anything is awaited the user's text is already on screen;
        // waiting to clear the box would make the field lag the list it just wrote to.
        draft.set(String::new());
        let user_id = user_id.clone();
        dispatch(&ui, toasts, move |app| async move {
            log("ui action create_todo");
            app.create(&user_id, &title).await?;
            Ok(None)
        });
    });

    rsx! {
        div { class: "compose",
            input {
                value: "{draft}",
                placeholder: "what needs doing",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| {
                    if event.key() == Key::Enter {
                        submit(());
                    }
                },
            }
            button { onclick: move |_| submit(()), "Add" }
        }
    }
}

/// One user's todos.
#[component]
pub fn TodoList(user_id: String) -> Element {
    let ui = use_context::<Ui>();
    // Reading the counter is the subscription; the value is not used for anything else. Without
    // this line the list renders once and then ignores every write, because `TodoStore` is a
    // `RefCell` and Dioxus cannot see into one. See `Ui::revision`.
    ui.revision.read();
    // `saving` is passed as a prop, not read inside `TodoRow`: the row's `todo` value may be
    // unchanged after a drain, and a skipped row would otherwise keep rendering the old marker.
    let rows: Vec<_> = ui
        .app
        .store()
        .rows_for(&user_id)
        .into_iter()
        .map(|todo| {
            let saving = ui.app.store().is_saving(&todo.id);
            (todo, saving)
        })
        .collect();

    rsx! {
        ul { class: "rows",
            for (todo, saving) in rows {
                TodoRow { key: "{todo.id}", todo, saving }
            }
        }
    }
}

/// One todo: tick it, rename it, delete it.
#[component]
fn TodoRow(todo: Todo, saving: bool) -> Element {
    let ui = use_context::<Ui>();
    let toasts = use_context::<Toasts>();

    let toggle = {
        let (ui, id, done) = (ui.clone(), todo.id.clone(), !todo.done);
        move |_| {
            let id = id.clone();
            dispatch(&ui, toasts, move |app| async move {
                log("ui action toggle_todo");
                // The one write that tries the server before the queue, and the only one whose
                // outcome is worth showing: a refusal here is the server declining on the merits,
                // and it will never reach the dead-letter panel because it was never queued.
                if let Direct::Refused(refusal) = app.set_done(&id, done).await? {
                    // Surfaced by hand rather than returned, because `dispatch` reads an `Ok`
                    // message as a success and this is the server declining on the merits. It will
                    // never reach the dead-letter panel either, because it was never queued.
                    toasts.push(Kind::Error, refusal.message);
                }
                Ok(None)
            });
        }
    };

    let rename = {
        let (ui, id, current_title) = (ui.clone(), todo.id.clone(), todo.title.clone());
        // `onchange` rather than `oninput`: every call enqueues a durable mutation, so renaming on
        // each keystroke would put one queued write per character into an outbox that may not
        // drain for hours.
        move |event: Event<FormData>| {
            let (id, title) = (id.clone(), event.value());
            if title == current_title {
                return;
            }
            dispatch(&ui, toasts, move |app| async move {
                log("ui action rename_todo");
                app.rename(&id, &title).await?;
                Ok(None)
            });
        }
    };

    let remove = {
        let (ui, id) = (ui.clone(), todo.id.clone());
        move |_| {
            let id = id.clone();
            dispatch(&ui, toasts, move |app| async move {
                log("ui action delete_todo");
                app.delete(&id).await?;
                Ok(None)
            });
        }
    };

    // **Every control in this row is nameless without these.** `×` is decoration, and an
    // `<input>`'s *value* is not its label — a screen reader reading this list unnamed announces
    // "checkbox, edit text, button" per row and never says which todo. The title goes in each name
    // because the row is the only thing distinguishing one set of controls from the next.
    let done_label = format!("Done: {}", todo.title);
    let rename_label = format!("Rename {}", todo.title);
    let delete_label = format!("Delete {}", todo.title);

    rsx! {
        li { class: if saving { "row saving" } else { "row" },
            input {
                r#type: "checkbox",
                "aria-label": "{done_label}",
                checked: todo.done,
                onchange: toggle,
            }
            input {
                class: "title",
                "aria-label": "{rename_label}",
                value: "{todo.title}",
                onchange: rename,
            }
            if saving {
                span { class: "mark", "saving…" }
            }
            button { "aria-label": "{delete_label}", onclick: remove, "×" }
        }
    }
}
