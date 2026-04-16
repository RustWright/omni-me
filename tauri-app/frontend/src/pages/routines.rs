use chrono_tz::Tz;
use dioxus::prelude::*;

use crate::bridge;
use crate::types::{CompletionEntry, RoutineGroup, RoutineItem};
use crate::user_date::UserDate;

#[derive(Clone, PartialEq)]
enum RoutineView {
    DailyChecklist,
    GroupList,
    GroupDetail(String),
    AddGroup,
    EditGroup(String),
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

            match &*view.read() {
                RoutineView::DailyChecklist => rsx! {
                    DailyChecklistView {
                        groups: groups.read().clone(),
                        on_manage: move |_| view.set(RoutineView::GroupList),
                    }
                },
                RoutineView::GroupList => rsx! {
                    GroupListView {
                        groups: groups.read().clone(),
                        on_add: move |_| view.set(RoutineView::AddGroup),
                        on_select: move |id: String| view.set(RoutineView::GroupDetail(id)),
                        on_back: move |_| view.set(RoutineView::DailyChecklist),
                    }
                },
                RoutineView::GroupDetail(id) => rsx! {
                    GroupDetailView {
                        group_id: id.clone(),
                        groups: groups.read().clone(),
                        on_edit: move |id: String| view.set(RoutineView::EditGroup(id)),
                        on_back: move |_| view.set(RoutineView::GroupList),
                    }
                },
                RoutineView::AddGroup => rsx! {
                    AddGroupView {
                        on_save: move |_| {
                            view.set(RoutineView::GroupList);
                            refresh_groups();
                        },
                        on_cancel: move |_| view.set(RoutineView::GroupList),
                    }
                },
                RoutineView::EditGroup(id) => rsx! {
                    EditGroupView {
                        group_id: id.clone(),
                        groups: groups.read().clone(),
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

// --- Daily Checklist View ---

#[component]
fn DailyChecklistView(
    groups: Vec<RoutineGroup>,
    on_manage: EventHandler<()>,
) -> Element {
    let tz_signal: Signal<Tz> = use_context();
    let today = UserDate::today(&*tz_signal.read()).to_date_string();

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

            if groups.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    svg { class: "w-16 h-16 mb-4 opacity-20", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                        path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "1", d: "M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-6 9l2 2 4-4" }
                    }
                    p { class: "text-lg font-medium", "No routines defined" }
                    p { class: "text-sm", "Tap \"Manage\" to build your flow" }
                }
            } else {
                for tod in &["morning", "afternoon", "evening"] {
                    {
                        let tod_groups: Vec<_> = groups.iter()
                            .filter(|g| g.time_of_day == *tod)
                            .cloned()
                            .collect();
                        if !tod_groups.is_empty() {
                            let (label, icon_path) = match *tod {
                                "morning" => ("Morning", "M12 3v1m0 16v1m9-9h-1M4 12H3m15.364-6.364l-.707.707M6.343 17.657l-.707.707m12.728 0l-.707-.707M6.343 6.343l-.707-.707M12 8a4 4 0 100 8 4 4 0 000-8z"),
                                "afternoon" => ("Afternoon", "M12 3v1m0 16v1m9-9h-1M4 12H3m15.364-6.364l-.707.707M6.343 17.657l-.707.707m12.728 0l-.707-.707M6.343 6.343l-.707-.707M12 8a4 4 0 100 8 4 4 0 000-8z"),
                                _ => ("Evening", "M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"),
                            };
                            let today = today.clone();
                            rsx! {
                                div { class: "mb-8",
                                    h3 { class: "flex items-center gap-2 text-[11px] font-bold text-obsidian-text-muted uppercase tracking-[0.2em] mb-4 ml-1",
                                        svg { class: "w-4 h-4 text-obsidian-accent opacity-70", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "2", d: icon_path }
                                        }
                                        "{label}"
                                    }
                                    div { class: "space-y-3",
                                        for group in tod_groups {
                                            ChecklistGroup { group, date: today.clone() }
                                        }
                                    }
                                }
                            }
                        } else {
                            rsx! {}
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
                items.set(list);
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
        .filter(|item| {
            completions_read
                .iter()
                .any(|c| c.item_id == item.id)
        })
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
                span { class: "font-bold text-[15px] tracking-tight text-white", "{group.name}" }
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
                                        onclick: move |_| {},
                                        svg { class: "w-4 h-4", fill: "none", stroke: "currentColor", view_box: "0 0 24 24",
                                            path { stroke_linecap: "round", stroke_linejoin: "round", stroke_width: "3", d: "M5 13l4 4L19 7" }
                                        }
                                    }
                                    span { class: "flex-1 text-sm text-obsidian-text-muted line-through opacity-50", "{item.name}" }
                                } else if is_skipped {
                                    div { class: "w-6 h-6 rounded-md bg-obsidian-text-muted/20 flex items-center justify-center text-obsidian-text-muted/40",
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
) -> Element {
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

            if groups.is_empty() {
                div { class: "flex flex-col items-center justify-center py-20 text-obsidian-text-muted",
                    p { "Your library is empty. Start by creating a group." }
                }
            } else {
                div { class: "grid gap-3",
                    for group in &groups {
                        {
                            let id = group.id.clone();
                            rsx! {
                                div {
                                    class: "p-4 bg-obsidian-sidebar/40 border border-white/5 rounded-lg cursor-pointer transition-all hover:bg-white/5 hover:border-white/10 active:scale-[0.98] flex justify-between items-center",
                                    onclick: move |_| on_select.call(id.clone()),
                                    div {
                                        span { class: "font-bold text-obsidian-text", "{group.name}" }
                                    }
                                    div { class: "flex items-center gap-2",
                                        span { class: "px-2 py-0.5 bg-obsidian-accent/10 text-obsidian-accent border border-obsidian-accent/20 rounded text-[10px] font-bold uppercase tracking-wider",
                                            "{group.frequency}"
                                        }
                                        span { class: "px-2 py-0.5 bg-white/5 text-obsidian-text-muted rounded text-[10px] font-bold uppercase tracking-wider",
                                            "{group.time_of_day}"
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
fn AddGroupView(
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut frequency = use_signal(|| "daily".to_string());
    let mut time_of_day = use_signal(|| "morning".to_string());
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
                        spawn(async move {
                            let n = name.read().clone();
                            let f = frequency.read().clone();
                            let t = time_of_day.read().clone();
                            match bridge::invoke_create_routine_group(&n, &f, &t).await {
                                Ok(_) => on_save.call(()),
                                Err(e) => save_error.set(Some(e)),
                            }
                            saving.set(false);
                        });
                    },
                    if *saving.read() { "Saving..." } else { "Save" }
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

                div { class: "grid grid-cols-2 gap-4",
                    div {
                        label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Frequency" }
                        select {
                            class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                            value: "{frequency}",
                            onchange: move |e| frequency.set(e.value()),
                            option { value: "daily", "Daily" }
                            option { value: "weekly", "Weekly" }
                            option { value: "custom", "Custom" }
                        }
                    }
                    div {
                        label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Focus Window" }
                        select {
                            class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                            value: "{time_of_day}",
                            onchange: move |e| time_of_day.set(e.value()),
                            option { value: "morning", "Morning" }
                            option { value: "afternoon", "Afternoon" }
                            option { value: "evening", "Evening" }
                        }
                    }
                }
            }
        }
    }
}

// --- Edit Group View ---

#[component]
fn EditGroupView(
    group_id: String,
    groups: Vec<RoutineGroup>,
    on_save: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let group = groups.iter().find(|g| g.id == group_id);
    let (init_name, init_freq, init_tod) = group
        .map(|g| (g.name.clone(), g.frequency.clone(), g.time_of_day.clone()))
        .unwrap_or_default();

    let mut name = use_signal(|| init_name);
    let mut frequency = use_signal(|| init_freq);
    let mut time_of_day = use_signal(|| init_tod);
    let mut saving = use_signal(|| false);

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
                h2 { class: "text-lg font-bold text-obsidian-text", "Edit Group" }
                button {
                    class: "px-4 py-1.5 bg-obsidian-accent text-white font-bold rounded-md hover:opacity-90 transition-opacity disabled:opacity-50",
                    disabled: *saving.read() || name.read().trim().is_empty(),
                    onclick: {
                        let gid = group_id.clone();
                        move |_| {
                            let gid = gid.clone();
                            saving.set(true);
                            spawn(async move {
                                let changes = serde_json::json!({
                                    "name": *name.read(),
                                    "frequency": *frequency.read(),
                                    "time_of_day": *time_of_day.read(),
                                });
                                if bridge::invoke_modify_routine_group(&gid, &changes, None).await.is_ok() {
                                    on_save.call(());
                                }
                                saving.set(false);
                            });
                        }
                    },
                    if *saving.read() { "Saving..." } else { "Save" }
                }
            }

            div { class: "space-y-6",
                div {
                    label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Group Name" }
                    input {
                        class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text placeholder-obsidian-text-muted outline-none focus:border-obsidian-accent transition-colors",
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "grid grid-cols-2 gap-4",
                    div {
                        label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Frequency" }
                        select {
                            class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                            value: "{frequency}",
                            onchange: move |e| frequency.set(e.value()),
                            option { value: "daily", "Daily" }
                            option { value: "weekly", "Weekly" }
                            option { value: "custom", "Custom" }
                        }
                    }
                    div {
                        label { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-2 block", "Focus Window" }
                        select {
                            class: "w-full px-4 py-2 bg-obsidian-sidebar border border-white/10 rounded-lg text-obsidian-text outline-none focus:border-obsidian-accent transition-colors appearance-none",
                            value: "{time_of_day}",
                            onchange: move |e| time_of_day.set(e.value()),
                            option { value: "morning", "Morning" }
                            option { value: "afternoon", "Afternoon" }
                            option { value: "evening", "Evening" }
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
    on_edit: EventHandler<String>,
    on_back: EventHandler<()>,
) -> Element {
    let group = groups.iter().find(|g| g.id == group_id).cloned();
    let group_name = group.as_ref().map(|g| g.name.clone()).unwrap_or_default();

    let mut items = use_signal(Vec::<RoutineItem>::new);
    let mut history = use_signal(Vec::<CompletionEntry>::new);
    let mut new_item_name = use_signal(String::new);
    let mut new_item_duration = use_signal(|| "5".to_string());
    let mut adding = use_signal(|| false);

    let gid = group_id.clone();
    let _load = use_future(move || {
        let gid = gid.clone();
        async move {
            if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                items.set(list);
            }
            if let Ok(list) = bridge::invoke_get_routine_history(&gid, 7).await {
                history.set(list);
            }
        }
    });

    let gid_for_edit = group_id.clone();

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
                button {
                    class: "px-3 py-1.5 bg-obsidian-sidebar border border-white/5 rounded-md hover:bg-white/5 text-obsidian-text text-sm transition-colors",
                    onclick: move |_| on_edit.call(gid_for_edit.clone()),
                    "Edit"
                }
            }

            h3 { class: "text-[10px] font-bold text-obsidian-text-muted uppercase tracking-widest mb-4 ml-1", "Step Configuration" }

            div { class: "space-y-1 mb-8",
                if items.read().is_empty() {
                    p { class: "text-sm text-obsidian-text-muted italic px-2", "No steps added yet." }
                } else {
                    for item in items.read().iter() {
                        div { class: "px-4 py-3 bg-obsidian-sidebar/20 border border-white/5 rounded-lg flex justify-between items-center",
                            span { class: "text-sm font-medium text-obsidian-text", "{item.name}" }
                            span { class: "text-[10px] font-mono text-obsidian-text-muted", "{item.estimated_duration_min}m" }
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
                        value: "{new_item_duration}",
                        oninput: move |e| new_item_duration.set(e.value()),
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
                                    let dur: u32 = new_item_duration.read().parse().unwrap_or(5);
                                    let order = items.read().len() as u32;
                                    if bridge::invoke_add_routine_item(&gid, &name, dur, order).await.is_ok() {
                                        new_item_name.set(String::new());
                                        if let Ok(list) = bridge::invoke_list_routine_items(&gid).await {
                                            items.set(list);
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
    let days: Vec<String> = (0..7)
        .rev()
        .map(|i| UserDate::days_ago(&tz, i).to_date_string())
        .collect();

    let day_labels: Vec<String> = (0..7)
        .rev()
        .map(|i| UserDate::days_ago(&tz, i).format("%a"))
        .collect();

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
