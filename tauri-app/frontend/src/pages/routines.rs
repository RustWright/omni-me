use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::bridge;
use crate::duration;
use crate::reorder;
use crate::types::{CompletionEntry, RoutineGroup, RoutineItem};
use crate::user_date::UserDate;

#[derive(Clone, PartialEq)]
enum RoutineView {
    DailyChecklist,
    GroupList,
    GroupDetail(String),
    AddGroup,
}

#[component]
pub fn RoutinesPage() -> Element {
    let mut view = use_signal(|| RoutineView::DailyChecklist);
    let mut groups = use_signal(Vec::<RoutineGroup>::new);
    let mut error_msg = use_signal(|| None::<String>);

    let _load = use_future(move || async move {
        match bridge::invoke_list_routine_groups().await {
            Ok(list) => groups.set(list),
            Err(e) => error_msg.set(Some(e)),
        }
    });

    let refresh_groups = move || {
        spawn(async move {
            match bridge::invoke_list_routine_groups().await {
                Ok(list) => {
                    groups.set(list);
                    error_msg.set(None);
                }
                Err(e) => error_msg.set(Some(e)),
            }
        });
    };

    rsx! {
        div { class: "max-w-3xl mx-auto w-full",

            if let Some(err) = &*error_msg.read() {
                div { class: "bg-red-900/20 text-red-400 px-3 py-2 rounded border border-red-900/50 mb-4 text-sm",
                    "{err}"
                }
            }

            {
                let visible: Vec<RoutineGroup> = groups
                    .read()
                    .iter()
                    .filter(|g| !g.removed)
                    .cloned()
                    .collect();

                match &*view.read() {
                    RoutineView::DailyChecklist => rsx! {
                        DailyChecklistView {
                            groups: visible,
                            on_manage: move |_| view.set(RoutineView::GroupList),
                        }
                    },
                    RoutineView::GroupList => rsx! {
                        GroupListView {
                            groups: visible,
                            on_add: move |_| view.set(RoutineView::AddGroup),
                            on_select: move |id: String| view.set(RoutineView::GroupDetail(id)),
                            on_back: move |_| view.set(RoutineView::DailyChecklist),
                            on_remove: move |id: String| {
                                spawn(async move {
                                    let _ = bridge::invoke_remove_routine_group(&id).await;
                                    refresh_groups();
                                });
                            },
                        }
                    },
                    RoutineView::GroupDetail(id) => rsx! {
                        GroupDetailView {
                            group_id: id.clone(),
                            groups: visible,
                            on_back: move |_| view.set(RoutineView::GroupList),
                        }
                    },
                    RoutineView::AddGroup => rsx! {
                        AddGroupView {
                            next_order: visible.len() as u32,
                            on_save: move |_| {
                                view.set(RoutineView::GroupList);
                                refresh_groups();
                            },
                            on_cancel: move |_| view.set(RoutineView::GroupList),
                        }
                    },
                }
            }
        }
    }
}

// --- Daily Checklist View ---
// Phase 0 dropped time-of-day buckets; this now renders groups as a flat
// user-ordered list. Phase 6.8/6.9 will add drag-to-reorder on top.

#[component]
fn DailyChecklistView(groups: Vec<RoutineGroup>, on_manage: EventHandler<()>) -> Element {
    let tz_signal: Signal<Tz> = use_context();
    // `&*signal.read()` is explicit on purpose: makes it clear we're
    // borrowing through a signal guard, not coercing the guard itself.
    #[allow(clippy::explicit_auto_deref)]
    let today = UserDate::today(&*tz_signal.read()).to_date_string();

    // The prop is the source of truth for "what the server knows". `pending_order`
    // is an optimistic override that the drop handler sets so the UI updates
    // without waiting for a DB roundtrip. When None, we sort the prop fresh.
    // Seeding the signal directly from the prop would lose the parent's later
    // load (prop is empty on first render while `use_future` is in flight).
    let mut pending_order: Signal<Option<Vec<RoutineGroup>>> = use_signal(|| None);
    let mut dragging_id = use_signal(|| None::<String>);
    let mut drag_over_id = use_signal(|| None::<String>);

    let ordered: Vec<RoutineGroup> = match pending_order.read().as_ref() {
        Some(o) => o.clone(),
        None => {
            let mut g = groups.clone();
            g.sort_by_key(|x| (x.order_num, x.name.clone()));
            g
        }
    };

    rsx! {
        div { class: "animate-in fade-in duration-300",
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold tracking-tight text-obsidian-accent", "Daily Flow" }
                button {
                    class: "px-3 py-1.5 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text text-sm transition-colors",
                    onclick: move |_| on_manage.call(()),
                    "Manage"
                }
            }

            if ordered.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    svg { class: "w-16 h-16 mb-4 opacity-20", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" }
                    }
                    p { class: "text-lg font-medium", "No routines defined" }
                    p { class: "text-sm", "Tap \"Manage\" to build your flow" }
                }
            } else {
                div { class: "space-y-3",
                    for group in ordered.iter().cloned().collect::<Vec<_>>() {
                        {
                            let id_start = group.id.clone();
                            let id_over = group.id.clone();
                            let id_leave = group.id.clone();
                            let id_drop = group.id.clone();
                            // Snapshot at render time — the next drop re-renders
                            // with fresh order, so there's no staleness risk.
                            let ordered_snapshot = ordered.clone();
                            let is_dragged = dragging_id.read().as_deref() == Some(group.id.as_str());
                            let is_hovered = {
                                let d = drag_over_id.read();
                                let g = dragging_id.read();
                                d.as_deref() == Some(group.id.as_str())
                                    && g.as_deref() != Some(group.id.as_str())
                                    && g.is_some()
                            };
                            let wrapper_class = format!(
                                "transition-all {} {}",
                                if is_dragged { "opacity-40 scale-[0.98]" } else { "" },
                                if is_hovered { "ring-2 ring-obsidian-accent ring-offset-2 ring-offset-obsidian-bg rounded-xl" } else { "" },
                            );
                            rsx! {
                                div {
                                    class: "{wrapper_class}",
                                    draggable: true,
                                    style: "cursor: grab",
                                    ondragstart: move |_| dragging_id.set(Some(id_start.clone())),
                                    ondragover: move |e| {
                                        // Calling prevent_default on dragover is what
                                        // tells the browser "this element accepts drops".
                                        // Without it, ondrop never fires.
                                        e.prevent_default();
                                        drag_over_id.set(Some(id_over.clone()));
                                    },
                                    ondragleave: move |_| {
                                        if drag_over_id.read().as_deref() == Some(id_leave.as_str()) {
                                            drag_over_id.set(None);
                                        }
                                    },
                                    ondrop: move |e| {
                                        e.prevent_default();
                                        let dragged = dragging_id.read().clone();
                                        dragging_id.set(None);
                                        drag_over_id.set(None);
                                        if let Some(dragged) = dragged {
                                            let new_order = reorder::reorder_groups_after_drop(
                                                ordered_snapshot.clone(),
                                                &dragged,
                                                &id_drop,
                                            );
                                            let payload = reorder::to_orderings_payload(&new_order);
                                            pending_order.set(Some(new_order));
                                            spawn(async move {
                                                let _ = bridge::invoke_reorder_routine_groups(&payload).await;
                                            });
                                        }
                                    },
                                    ondragend: move |_| {
                                        dragging_id.set(None);
                                        drag_over_id.set(None);
                                    },
                                    ChecklistGroup { group: group.clone(), date: today.clone() }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ChecklistGroup(group: RoutineGroup, date: String) -> Element {
    let mut items = use_signal(Vec::<RoutineItem>::new);
    let mut completions = use_signal(Vec::<CompletionEntry>::new);
    let group_id = group.id.clone();
    let date_for_load = date.clone();

    let _load = use_future(move || {
        let gid = group_id.clone();
        let d = date_for_load.clone();
        async move {
            if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                items.set(list.into_iter().filter(|i| !i.removed).collect());
            }
            if let Ok(list) = bridge::invoke_get_completions_for_date(&gid, &d).await {
                completions.set(list);
            }
        }
    });

    let items_read = items.read();
    let completions_read = completions.read();
    let done_count = items_read
        .iter()
        .filter(|item| completions_read.iter().any(|c| c.item_id == item.id))
        .count();
    let total = items_read.len();
    let is_fully_done = total > 0 && done_count == total;

    let base_class = "bg-obsidian-sidebar/40 border border-white/5 rounded-xl overflow-hidden shadow-sm transition-all";
    let status_class = if is_fully_done { "opacity-60 grayscale-[0.5]" } else { "" };
    let container_class = format!("{} {}", base_class, status_class);

    let progress_width = if total > 0 {
        (done_count as f32 / total as f32) * 100.0
    } else {
        0.0
    };
    let progress_style = format!("width: {}%", progress_width);

    rsx! {
        div { class: "{container_class}",
            div { class: "px-4 py-3 bg-white/5 flex justify-between items-center border-bottom border-white/5",
                div { class: "flex items-center gap-2",
                    span { class: "font-bold text-[15px] tracking-tight text-white", "{group.name}" }
                    span { class: "px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-[10px] font-bold uppercase tracking-wider",
                        "{group.frequency}"
                    }
                }
                div { class: "flex items-center gap-2",
                    div { class: "w-16 h-1.5 bg-obsidian-bg rounded-full overflow-hidden",
                        div {
                            class: "h-full bg-obsidian-accent transition-all duration-500",
                            style: "{progress_style}"
                        }
                    }
                    span { class: "text-[11px] font-mono text-obsidian-text-muted", "{done_count}/{total}" }
                }
            }

            div { class: "divide-y divide-white/5",
                for item in items_read.iter() {
                    {
                        let completion = completions_read.iter().find(|c| c.item_id == item.id);
                        let is_done = completion.is_some();
                        let is_skipped = completion.map(|c| c.skipped).unwrap_or(false);
                        let item_id = item.id.clone();
                        let gid = group.id.clone();
                        let d = date.clone();

                        rsx! {
                            div { class: "px-4 py-3 flex items-center gap-3 group transition-colors hover:bg-white/[0.02]",

                                if is_done && !is_skipped {
                                    button {
                                        class: "w-6 h-6 rounded-md bg-obsidian-accent flex items-center justify-center text-white transition-all scale-110",
                                        onclick: {
                                            let iid = item_id.clone();
                                            let gid_inner = gid.clone();
                                            let d_inner = d.clone();
                                            move |_| {
                                                let iid = iid.clone();
                                                let gid_inner = gid_inner.clone();
                                                let d_inner = d_inner.clone();
                                                spawn(async move {
                                                    let _ = bridge::invoke_undo_completion(&iid, &d_inner).await;
                                                    if let Ok(list) = bridge::invoke_get_completions_for_date(&gid_inner, &d_inner).await {
                                                        completions.set(list);
                                                    }
                                                });
                                            }
                                        },
                                        svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                        }
                                    }
                                    span { class: "flex-1 text-sm text-obsidian-text-muted line-through opacity-50", "{item.name}" }
                                } else if is_skipped {
                                    button {
                                        class: "w-6 h-6 rounded-md bg-obsidian-text-muted/20 flex items-center justify-center text-obsidian-text-muted/40",
                                        onclick: {
                                            let iid = item_id.clone();
                                            let gid_inner = gid.clone();
                                            let d_inner = d.clone();
                                            move |_| {
                                                let iid = iid.clone();
                                                let gid_inner = gid_inner.clone();
                                                let d_inner = d_inner.clone();
                                                spawn(async move {
                                                    let _ = bridge::invoke_undo_skip(&iid, &d_inner).await;
                                                    if let Ok(list) = bridge::invoke_get_completions_for_date(&gid_inner, &d_inner).await {
                                                        completions.set(list);
                                                    }
                                                });
                                            }
                                        },
                                        svg { class: "w-3 h-3", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M20 12H4" }
                                        }
                                    }
                                    span { class: "flex-1 text-sm text-obsidian-text-muted/40 italic", "{item.name} (skipped)" }
                                } else {
                                    button {
                                        class: "w-6 h-6 rounded-md border-2 border-obsidian-text-muted/30 bg-transparent hover:border-obsidian-accent transition-colors",
                                        onclick: {
                                            let iid = item_id.clone();
                                            let gid = gid.clone();
                                            let d = d.clone();
                                            move |_| {
                                                let iid = iid.clone();
                                                let gid = gid.clone();
                                                let d = d.clone();
                                                spawn(async move {
                                                    let _ = bridge::invoke_complete_routine_item(&iid, &gid, &d).await;
                                                    if let Ok(list) = bridge::invoke_get_completions_for_date(&gid, &d).await {
                                                        completions.set(list);
                                                    }
                                                });
                                            }
                                        },
                                    }
                                    span { class: "flex-1 text-sm font-medium text-obsidian-text group-hover:text-white transition-colors", "{item.name}" }
                                    button {
                                        class: "px-2 py-1 bg-white/5 border border-white/5 rounded text-[10px] font-bold text-obsidian-text-muted hover:text-white transition-colors opacity-0 group-hover:opacity-100",
                                        onclick: {
                                            let iid = item_id.clone();
                                            let gid = gid.clone();
                                            let d = d.clone();
                                            move |_| {
                                                let iid = iid.clone();
                                                let gid = gid.clone();
                                                let d = d.clone();
                                                spawn(async move {
                                                    let _ = bridge::invoke_skip_routine_item(&iid, &gid, &d, None).await;
                                                    if let Ok(list) = bridge::invoke_get_completions_for_date(&gid, &d).await {
                                                        completions.set(list);
                                                    }
                                                });
                                            }
                                        },
                                        "SKIP"
                                    }
                                }

                                if item.estimated_duration_min > 0 {
                                    span { class: "text-[10px] font-mono text-obsidian-text-muted/40",
                                        "{item.estimated_duration_min}m"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// --- Group List View ---

#[component]
fn GroupListView(
    groups: Vec<RoutineGroup>,
    on_add: EventHandler<()>,
    on_select: EventHandler<String>,
    on_back: EventHandler<()>,
    on_remove: EventHandler<String>,
) -> Element {
    let mut pending_remove = use_signal(|| None::<String>);
    let mut ordered = groups;
    ordered.sort_by_key(|g| (g.order_num, g.name.clone()));

    rsx! {
        div { class: "animate-in slide-in-from-right-4 duration-300",
            div { class: "flex justify-between items-center mb-6",
                div { class: "flex items-center gap-3",
                    button {
                        class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors",
                        onclick: move |_| on_back.call(()),
                        svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M10 19l-7-7m0 0l7-7m-7 7h18" }
                        }
                    }
                    h1 { class: "text-2xl font-bold tracking-tight text-obsidian-text", "Routine Library" }
                }
                button {
                    class: "flex items-center gap-2 px-4 py-2 bg-obsidian-accent text-white font-semibold rounded-md hover:opacity-90 transition-opacity shadow-lg shadow-obsidian-accent/20",
                    onclick: move |_| on_add.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M12 4v16m8-8H4" }
                    }
                    span { "New Group" }
                }
            }

            if ordered.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    p { "Your library is empty. Start by creating a group." }
                }
            } else {
                div { class: "grid gap-3",
                    for group in &ordered {
                        {
                            let id_select = group.id.clone();
                            let id_arm = group.id.clone();
                            let id_confirm = group.id.clone();
                            let is_pending = pending_remove.read().as_deref() == Some(group.id.as_str());
                            rsx! {
                                div { class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg transition-all hover:bg-white/5 hover:border-white/10 flex justify-between items-center",
                                    div { class: "flex-1 cursor-pointer",
                                        onclick: move |_| on_select.call(id_select.clone()),
                                        span { class: "font-bold text-obsidian-text", "{group.name}" }
                                    }
                                    div { class: "flex items-center gap-2",
                                        span { class: "px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-[10px] font-bold uppercase tracking-wider",
                                            "{group.frequency}"
                                        }
                                        if is_pending {
                                            button {
                                                class: "px-2 py-1 bg-red-600 text-white border border-red-700 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-red-500 transition-colors",
                                                onclick: move |_| {
                                                    on_remove.call(id_confirm.clone());
                                                    pending_remove.set(None);
                                                },
                                                "Confirm?"
                                            }
                                            button {
                                                class: "px-2 py-1 bg-white/5 text-obsidian-text border border-white/10 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-white/10 transition-colors",
                                                onclick: move |_| pending_remove.set(None),
                                                "Cancel"
                                            }
                                        } else {
                                            button {
                                                class: "px-2 py-1 bg-red-900/20 text-red-400 border border-red-900/30 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-red-900/30 transition-colors",
                                                onclick: move |_| pending_remove.set(Some(id_arm.clone())),
                                                "Remove"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// --- Add Group View ---

#[component]
fn AddGroupView(next_order: u32, on_save: EventHandler<()>, on_cancel: EventHandler<()>) -> Element {
    let mut name = use_signal(String::new);
    // "daily" | "weekly" | "biweekly" | "monthly" | "custom"
    let mut frequency = use_signal(|| "daily".to_string());
    let mut custom_n = use_signal(|| 3u32);
    let mut saving = use_signal(|| false);
    let mut save_error = use_signal(|| None::<String>);

    rsx! {
        div { class: "animate-in fade-in slide-in-from-bottom-4 duration-300",
            div { class: "flex justify-between items-center mb-6",
                button {
                    class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors",
                    onclick: move |_| on_cancel.call(()),
                    svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M6 18L18 6M6 6l12 12" }
                    }
                }
                h2 { class: "text-lg font-bold text-obsidian-text", "New Group" }
                button {
                    class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50",
                    disabled: *saving.read() || name.read().trim().is_empty(),
                    onclick: move |_| {
                        saving.set(true);
                        save_error.set(None);
                        spawn(async move {
                            let n = name.read().clone();
                            let f_raw = frequency.read().clone();
                            let f_wire = if f_raw == "custom" {
                                format!("custom:{}", *custom_n.read())
                            } else {
                                f_raw
                            };
                            match bridge::invoke_create_routine_group(&n, &f_wire, next_order).await {
                                Ok(_) => on_save.call(()),
                                Err(e) => save_error.set(Some(e)),
                            }
                            saving.set(false);
                        });
                    },
                    if *saving.read() { "Saving..." } else { "Save" }
                }
            }

            if let Some(err) = &*save_error.read() {
                div { class: "mb-4 p-3 bg-red-900/20 text-red-400 rounded border border-red-900/50 text-sm",
                    "{err}"
                }
            }

            div { class: "space-y-6",
                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Group Name" }
                    input {
                        class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "text",
                        placeholder: "e.g. Morning Ritual",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Frequency" }
                    select {
                        class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                        value: "{frequency}",
                        onchange: move |e| frequency.set(e.value()),
                        option { value: "daily", "Daily" }
                        option { value: "weekly", "Weekly" }
                        option { value: "biweekly", "Biweekly" }
                        option { value: "monthly", "Monthly" }
                        option { value: "custom", "Custom (every N days)" }
                    }

                    if *frequency.read() == "custom" {
                        div { class: "flex items-center gap-3 mt-3 animate-in fade-in duration-200",
                            span { class: "text-sm text-obsidian-text-muted", "every" }
                            input {
                                class: "w-20 px-3 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text text-center outline-none focus:border-obsidian-accent transition-colors",
                                r#type: "number",
                                min: "2",
                                max: "365",
                                value: "{custom_n}",
                                oninput: move |e| {
                                    // Parse + clamp to [2, 365]. Custom:1 would be identical
                                    // to Daily; upper bound is a sanity cap.
                                    if let Ok(n) = e.value().parse::<u32>() {
                                        custom_n.set(n.clamp(2, 365));
                                    }
                                },
                            }
                            span { class: "text-sm text-obsidian-text-muted", "days" }
                        }
                    }
                }
            }
        }
    }
}

// --- Group Detail View ---

#[component]
fn GroupDetailView(
    group_id: String,
    groups: Vec<RoutineGroup>,
    on_back: EventHandler<()>,
) -> Element {
    let group = groups.iter().find(|g| g.id == group_id).cloned();
    let group_name = group.as_ref().map(|g| g.name.clone()).unwrap_or_default();

    let mut items = use_signal(Vec::<RoutineItem>::new);
    let mut history = use_signal(Vec::<CompletionEntry>::new);
    let mut new_item_name = use_signal(String::new);
    let mut new_item_duration = use_signal(|| "5".to_string());
    let mut new_item_unit = use_signal(|| duration::UNIT_MIN.to_string());
    let mut adding = use_signal(|| false);
    let mut pending_remove = use_signal(|| None::<String>);

    // Inline-edit state. Only one item can be in edit mode at a time; entering
    // edit mode auto-clears any pending-remove arming on other rows.
    let mut editing_id = use_signal(|| None::<String>);
    let mut edit_name = use_signal(String::new);
    let mut edit_duration_value = use_signal(|| "0".to_string());
    let mut edit_duration_unit = use_signal(|| duration::UNIT_MIN.to_string());
    let mut saving_edit = use_signal(|| false);

    let gid = group_id.clone();
    let _load = use_future(move || {
        let gid = gid.clone();
        async move {
            if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                items.set(list.into_iter().filter(|i| !i.removed).collect());
            }
            if let Ok(list) = bridge::invoke_get_routine_history(&gid, 7).await {
                history.set(list);
            }
        }
    });

    rsx! {
        div { class: "animate-in fade-in duration-300",
            div { class: "flex justify-between items-center mb-6",
                div { class: "flex items-center gap-3",
                    button {
                        class: "p-2 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text transition-colors",
                        onclick: move |_| on_back.call(()),
                        svg { class: "w-5 h-5", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: "M10 19l-7-7m0 0l7-7m-7 7h18" }
                        }
                    }
                    h1 { class: "text-xl font-bold text-obsidian-text", "{group_name}" }
                }
            }

            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-4 ml-1", "Step Configuration" }

            div { class: "space-y-1 mb-8",
                if items.read().is_empty() {
                    p { class: "text-sm text-obsidian-text-muted italic px-2", "No steps added yet." }
                } else {
                    for item in items.read().iter() {
                        {
                            let id_arm = item.id.clone();
                            let id_confirm = item.id.clone();
                            let id_edit_arm = item.id.clone();
                            let id_edit_save = item.id.clone();
                            let item_dur = item.estimated_duration_min.max(0) as u32;
                            let item_name_for_edit = item.name.clone();
                            let group_for_reload = group_id.clone();
                            let group_for_edit = group_id.clone();
                            let is_pending = pending_remove.read().as_deref() == Some(item.id.as_str());
                            let is_editing = editing_id.read().as_deref() == Some(item.id.as_str());
                            rsx! {
                                if is_editing {
                                    div { class: "px-4 py-3 bg-obsidian-sidebar/40 border border-obsidian-accent/40 rounded-lg space-y-3 animate-in fade-in duration-150",
                                        div { class: "flex gap-2",
                                            input {
                                                class: "flex-1 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text outline-none focus:border-obsidian-accent transition-colors",
                                                r#type: "text",
                                                value: "{edit_name}",
                                                oninput: move |e| edit_name.set(e.value()),
                                            }
                                            input {
                                                class: "w-16 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text text-center outline-none focus:border-obsidian-accent transition-colors",
                                                r#type: "number",
                                                min: "0",
                                                value: "{edit_duration_value}",
                                                oninput: move |e| edit_duration_value.set(e.value()),
                                            }
                                            select {
                                                class: "w-20 px-2 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                                                value: "{edit_duration_unit}",
                                                onchange: move |e| edit_duration_unit.set(e.value()),
                                                option { value: "{duration::UNIT_MIN}", "min" }
                                                option { value: "{duration::UNIT_HOUR}", "hour" }
                                            }
                                        }
                                        div { class: "flex gap-2 justify-end",
                                            button {
                                                class: "px-3 py-1.5 bg-obsidian-accent text-white text-[11px] font-bold uppercase tracking-wider rounded hover:opacity-90 transition-opacity disabled:opacity-50",
                                                disabled: *saving_edit.read() || edit_name.read().trim().is_empty(),
                                                onclick: {
                                                    let gid = group_for_edit.clone();
                                                    move |_| {
                                                        let gid = gid.clone();
                                                        let iid = id_edit_save.clone();
                                                        let new_name = edit_name.read().trim().to_string();
                                                        let val: u32 = edit_duration_value.read().parse().unwrap_or(0);
                                                        let minutes = duration::to_minutes(val, &edit_duration_unit.read());
                                                        saving_edit.set(true);
                                                        spawn(async move {
                                                            let changes = serde_json::json!({
                                                                "name": new_name,
                                                                "estimated_duration_min": minutes,
                                                            });
                                                            let _ = bridge::invoke_modify_routine_item(&iid, &changes).await;
                                                            if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                                                                items.set(list.into_iter().filter(|i| !i.removed).collect());
                                                            }
                                                            editing_id.set(None);
                                                            saving_edit.set(false);
                                                        });
                                                    }
                                                },
                                                if *saving_edit.read() { "Saving…" } else { "Save" }
                                            }
                                            button {
                                                class: "px-3 py-1.5 bg-white/5 text-obsidian-text border border-white/10 text-[11px] font-bold uppercase tracking-wider rounded hover:bg-white/10 transition-colors",
                                                onclick: move |_| editing_id.set(None),
                                                "Cancel"
                                            }
                                        }
                                    }
                                } else {
                                    div { class: "px-4 py-3 bg-obsidian-sidebar/20 border border-white/5 rounded-lg flex justify-between items-center",
                                        span { class: "text-sm font-medium text-obsidian-text", "{item.name}" }
                                        div { class: "flex items-center gap-2",
                                            span { class: "text-[10px] font-mono text-obsidian-text-muted", "{item.estimated_duration_min}m" }
                                            if is_pending {
                                                button {
                                                    class: "px-2 py-1 bg-red-600 text-white border border-red-700 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-red-500 transition-colors",
                                                    onclick: move |_| {
                                                        let iid = id_confirm.clone();
                                                        let gid = group_for_reload.clone();
                                                        pending_remove.set(None);
                                                        spawn(async move {
                                                            let _ = bridge::invoke_remove_routine_item(&iid).await;
                                                            if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                                                                items.set(list.into_iter().filter(|i| !i.removed).collect());
                                                            }
                                                        });
                                                    },
                                                    "Confirm?"
                                                }
                                                button {
                                                    class: "px-2 py-1 bg-white/5 text-obsidian-text border border-white/10 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-white/10 transition-colors",
                                                    onclick: move |_| pending_remove.set(None),
                                                    "Cancel"
                                                }
                                            } else {
                                                button {
                                                    class: "px-2 py-1 bg-white/5 text-obsidian-text-muted border border-white/10 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-white/10 hover:text-white transition-colors",
                                                    onclick: move |_| {
                                                        let (val, unit) = duration::split_minutes_for_display(item_dur);
                                                        edit_name.set(item_name_for_edit.clone());
                                                        edit_duration_value.set(val.to_string());
                                                        edit_duration_unit.set(unit.to_string());
                                                        pending_remove.set(None);
                                                        editing_id.set(Some(id_edit_arm.clone()));
                                                    },
                                                    "Edit"
                                                }
                                                button {
                                                    class: "px-2 py-1 bg-red-900/20 text-red-400 border border-red-900/30 rounded text-[10px] font-bold uppercase tracking-wider hover:bg-red-900/30 transition-colors",
                                                    onclick: move |_| {
                                                        editing_id.set(None);
                                                        pending_remove.set(Some(id_arm.clone()));
                                                    },
                                                    "Remove"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-xl space-y-4",
                h4 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest", "Add New Step" }
                div { class: "flex gap-2",
                    input {
                        class: "flex-1 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "text",
                        placeholder: "Step name...",
                        value: "{new_item_name}",
                        oninput: move |e| new_item_name.set(e.value()),
                    }
                    input {
                        class: "w-16 px-3 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text text-center outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "number",
                        min: "0",
                        value: "{new_item_duration}",
                        oninput: move |e| new_item_duration.set(e.value()),
                    }
                    select {
                        class: "w-20 px-2 py-2 bg-obsidian-bg border border-white/10 rounded-lg text-sm text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                        value: "{new_item_unit}",
                        onchange: move |e| new_item_unit.set(e.value()),
                        option { value: "{duration::UNIT_MIN}", "min" }
                        option { value: "{duration::UNIT_HOUR}", "hour" }
                    }
                    button {
                        class: "px-4 py-2 bg-obsidian-accent text-white font-bold rounded-lg hover:opacity-90 transition-opacity disabled:opacity-50",
                        disabled: *adding.read() || new_item_name.read().trim().is_empty(),
                        onclick: {
                            let gid = group_id.clone();
                            move |_| {
                                let gid = gid.clone();
                                adding.set(true);
                                spawn(async move {
                                    let name = new_item_name.read().clone();
                                    let val: u32 = new_item_duration.read().parse().unwrap_or(5);
                                    let minutes = duration::to_minutes(val, &new_item_unit.read());
                                    let order = items.read().len() as u32;
                                    if bridge::invoke_add_routine_item(&gid, &name, minutes, order).await.is_ok() {
                                        new_item_name.set(String::new());
                                        new_item_duration.set("5".to_string());
                                        new_item_unit.set(duration::UNIT_MIN.to_string());
                                        if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                                            items.set(list.into_iter().filter(|i| !i.removed).collect());
                                        }
                                    }
                                    adding.set(false);
                                });
                            }
                        },
                        "Add"
                    }
                }
            }

            if !items.read().is_empty() {
                HistoryGrid { items: items.read().clone(), history: history.read().clone() }
            }
        }
    }
}

// --- History Grid ---

#[component]
fn HistoryGrid(items: Vec<RoutineItem>, history: Vec<CompletionEntry>) -> Element {
    let tz_signal: Signal<Tz> = use_context();
    let tz = *tz_signal.read();
    let days: Vec<String> = (0..7).rev().map(|i| UserDate::days_ago(&tz, i).to_date_string()).collect();

    let day_labels: Vec<String> = (0..7).rev().map(|i| UserDate::days_ago(&tz, i).format("%a")).collect();

    rsx! {
        div { class: "mt-12 animate-in fade-in duration-500",
            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-6 ml-1", "7-Day Performance" }

            div { class: "grid grid-cols-[1fr_repeat(7,32px)] gap-2 mb-4 px-2",
                div {}
                for label in &day_labels {
                    div { class: "text-[10px] font-bold text-obsidian-text-muted text-center", "{label}" }
                }
            }

            div { class: "space-y-2",
                for item in &items {
                    div { class: "grid grid-cols-[1fr_repeat(7,32px)] gap-2 items-center px-2",
                        div { class: "text-[12px] text-obsidian-text truncate pr-2", "{item.name}" }
                        for day in &days {
                            {
                                let completion = history.iter().find(|c| c.item_id == item.id && c.date == *day);
                                let is_skipped = completion.as_ref().map(|c| c.skipped).unwrap_or(false);
                                let is_done = completion.is_some();

                                let bg_class = if is_skipped {
                                    "bg-white/5 border-white/5 text-obsidian-text-muted/40"
                                } else if is_done {
                                    "bg-green-500/20 border-green-500/30 text-green-500"
                                } else {
                                    "bg-obsidian-sidebar border-white/5 text-transparent"
                                };

                                rsx! {
                                    div { class: "w-8 h-8 rounded-md border flex items-center justify-center text-[10px] font-bold {bg_class}",
                                        if is_skipped { "—" } else if is_done { "✓" } else { "" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
