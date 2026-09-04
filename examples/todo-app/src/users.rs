//! The user list, and the todo list inside each user.
//!
//! # Two levels, one pattern
//!
//! A user row is `[expander] [name] [tag] [count] [×]`; a todo row is
//! `[checkbox] [title] [mark] [×]`. They are deliberately the same shape at two depths, which is
//! why `rows::TodoRow` needed no changes at all to be nested inside one — it already did toggle,
//! rename and delete per row.
//!
//! # The list is the server's, not this client's creations
//!
//! `refresh_users_from_server` returns every user the service holds, and this renders all of them.
//! That is what puts D4d's invalidation runtime on screen: two browser windows against one pair of
//! servers converge after a poll, and nothing else on this page demonstrates
//! `InvalidationRunner::apply` doing anything at all. The cost is that you can delete a record
//! another window created, which for a demo is a feature.

use std::collections::HashSet;

use dioxus::prelude::*;
use todo_core::User;

use crate::toast::Toasts;
use crate::{dispatch, log, rows, Ui};

/// Every user, with a composer above them.
#[component]
pub fn UserList() -> Element {
    let ui = use_context::<Ui>();
    let toasts = use_context::<Toasts>();
    let mut draft = use_signal(String::new);

    // Your own row starts open, because it is the one you came here to use. Everyone else's is a
    // click away, which keeps the page short once a second window has added a few.
    let expanded = use_signal(|| HashSet::from([ui.app.user_id().to_owned()]));
    // At most one confirm open at a time: a page full of half-armed delete buttons is a page where
    // the next click is a guess.
    let confirming: Signal<Option<String>> = use_signal(|| None);

    // Subscribes this component to the projection. Without it the list renders once and then
    // ignores every write — see `Ui::revision`.
    ui.revision.read();
    let me = ui.app.user_id().to_owned();
    // Yourself first, then everyone else by name. `TodoStore::users` already sorts by name; this
    // lifts your own row out of that order, because it is the one you came here to use and hunting
    // for it alphabetically among a list other windows keep adding to is a poor way to find it.
    let mut users = ui.app.store().users();
    users.sort_by_key(|user| user.id != me);

    // `ui` is cloned in rather than re-read here, and the difference is a rule rather than a
    // preference. `use_context` is a hook — it is `use_hook(|| consume_context())` — so calling it
    // in a callback body runs a hook outside render, which appends to this scope's hook list on
    // every click. It did not panic, which is the trap: the extra slots land after the ones render
    // reads, so the damage stays invisible until a hook is added below this one. Every other
    // handler in this crate captures the value from render scope; this one now does too.
    let add = use_callback({
        let ui = ui.clone();
        move |()| {
            let name = draft.read().trim().to_owned();
            if name.is_empty() {
                return;
            }
            draft.set(String::new());
            dispatch(&ui, toasts, move |app| async move {
                log("ui action create_user");
                app.create_user(&name).await?;
                Ok(None)
            });
        }
    });

    rsx! {
        div { class: "compose",
            input {
                value: "{draft}",
                placeholder: "new user's name",
                oninput: move |event| draft.set(event.value()),
                onkeydown: move |event| {
                    if event.key() == Key::Enter {
                        add(());
                    }
                },
            }
            button { onclick: move |_| add(()), "Add user" }
        }

        if users.len() == 1 {
            p { class: "notice",
                "Just you so far. Add a user above to keep todos in someone else's name."
            }
        }

        ul { class: "users",
            for user in users {
                UserRow {
                    key: "{user.id}",
                    count: ui.app.store().count_for(&user.id),
                    is_me: user.id == me,
                    open: expanded.read().contains(&user.id),
                    expanded,
                    confirming,
                    user,
                }
            }
        }
    }
}

/// One user: rename them, delete them, or open their todos.
#[component]
fn UserRow(
    user: User,
    count: usize,
    is_me: bool,
    open: bool,
    expanded: Signal<HashSet<String>>,
    confirming: Signal<Option<String>>,
) -> Element {
    let ui = use_context::<Ui>();
    let toasts = use_context::<Toasts>();
    let armed = confirming.read().as_deref() == Some(user.id.as_str());

    let toggle = {
        let (id, mut expanded) = (user.id.clone(), expanded);
        move |_| {
            let mut open = expanded.write();
            if !open.remove(&id) {
                open.insert(id.clone());
            }
        }
    };

    let rename = {
        let (ui, id, current) = (ui.clone(), user.id.clone(), user.name.clone());
        // `onchange` rather than `oninput`, the same rule a todo rename follows: every call
        // enqueues a durable mutation, so renaming per keystroke would put one queued write per
        // character into an outbox that may not drain for hours.
        move |event: Event<FormData>| {
            let (id, name) = (id.clone(), event.value());
            if name == current || name.trim().is_empty() {
                return;
            }
            dispatch(&ui, toasts, move |app| async move {
                log("ui action rename_user");
                app.rename_user(&id, &name).await?;
                Ok(None)
            });
        }
    };

    let remove = {
        let (ui, id, mut confirming) = (ui.clone(), user.id.clone(), confirming);
        move |_| {
            confirming.set(None);
            let id = id.clone();
            dispatch(&ui, toasts, move |app| async move {
                log("ui action delete_user");
                app.delete_user(&id).await?;
                Ok(None)
            });
        }
    };

    let arm = {
        let (id, mut confirming) = (user.id.clone(), confirming);
        move |_| confirming.set(Some(id.clone()))
    };

    // See `crate::rows` for why these exist: a glyph and a value are not accessible names. The
    // expander's name states the action it will perform rather than the state it is in, because
    // that is what a person operating it is choosing.
    let expander_label = if open {
        format!("Collapse {}'s todos", user.name)
    } else {
        format!("Expand {}'s todos", user.name)
    };
    let rename_label = format!("Rename {}", user.name);
    let delete_label = format!("Delete {}", user.name);

    rsx! {
        li { class: "user",
            div { class: "user-head",
                button {
                    class: "expander",
                    "aria-label": "{expander_label}",
                    "aria-expanded": "{open}",
                    onclick: toggle,
                    if open { "▼" } else { "▶" }
                }
                input {
                    class: "title",
                    "aria-label": "{rename_label}",
                    value: "{user.name}",
                    onchange: rename,
                }
                if is_me {
                    span { class: "tag", "(you)" }
                }
                span { class: "count",
                    match count {
                        0 => "no todos".to_owned(),
                        1 => "1 todo".to_owned(),
                        n => format!("{n} todos"),
                    }
                }
                if armed {
                    // The count is the warning, so it goes on the button. Deleting a user destroys
                    // rows on a *different* service — see `TodoApp::delete_user`.
                    span { class: "confirm",
                        "delete {user.name} and {count} todos?"
                        button { onclick: remove, "yes" }
                        button { onclick: move |_| confirming.set(None), "no" }
                    }
                } else if count == 0 {
                    // Nothing to warn about, so no confirm step.
                    button { "aria-label": "{delete_label}", onclick: remove, "×" }
                } else {
                    button { "aria-label": "{delete_label}", onclick: arm, "×" }
                }
            }

            if open {
                div { class: "nested",
                    if count == 0 {
                        p { class: "notice", "nothing yet — add one below" }
                    }
                    rows::TodoList { user_id: user.id.clone() }
                    rows::Composer { user_id: user.id.clone() }
                }
            }
        }
    }
}
