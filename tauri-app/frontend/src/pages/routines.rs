use dioxus::prelude::*;

use crate::bridge;
use crate::types::{CompletionEntry, RoutineGroup, RoutineItem};

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
        div {
            style: "max-width: 720px; margin: 0 auto;",

            if let Some(err) = &*error_msg.read() {
                div {
                    style: "background: #fee; color: #c33; padding: 8px 12px; border-radius: 6px; margin-bottom: 12px; font-size: 14px;",
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

// --- Daily Checklist View (Task 6.3) ---

#[component]
fn DailyChecklistView(
    groups: Vec<RoutineGroup>,
    on_manage: EventHandler<()>,
) -> Element {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    rsx! {
        div {
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                h1 {
                    style: "font-size: 24px; font-weight: 600; margin: 0; color: #1a1a2e;",
                    "Today's Routines"
                }
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_manage.call(()),
                    "Manage"
                }
            }

            if groups.is_empty() {
                div {
                    style: "text-align: center; padding: 48px 16px; color: #888;",
                    p { style: "font-size: 18px; margin-bottom: 8px;", "No routines yet" }
                    p { style: "font-size: 14px;", "Tap \"Manage\" to create routine groups" }
                }
            } else {
                // Group by time_of_day
                for tod in &["morning", "afternoon", "evening"] {
                    {
                        let tod_groups: Vec<_> = groups.iter()
                            .filter(|g| g.time_of_day == *tod)
                            .cloned()
                            .collect();
                        if !tod_groups.is_empty() {
                            let label = match *tod {
                                "morning" => "Morning",
                                "afternoon" => "Afternoon",
                                _ => "Evening",
                            };
                            let today = today.clone();
                            rsx! {
                                div {
                                    style: "margin-bottom: 20px;",
                                    h3 {
                                        style: "font-size: 13px; font-weight: 600; color: #888; text-transform: uppercase; letter-spacing: 0.5px; margin: 0 0 8px 0;",
                                        "{label}"
                                    }
                                    for group in tod_groups {
                                        ChecklistGroup { group, date: today.clone() }
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

    rsx! {
        div {
            style: "background: white; border-radius: 8px; border: 1px solid #eee; margin-bottom: 8px; overflow: hidden;",

            // Group header with progress
            div {
                style: "padding: 10px 12px; display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid #f0f0f0;",
                span { style: "font-weight: 600; font-size: 15px;", "{group.name}" }
                span {
                    style: "font-size: 13px; color: #888;",
                    "{done_count}/{total}"
                }
            }

            // Items
            for item in items_read.iter() {
                {
                    let completion = completions_read.iter().find(|c| c.item_id == item.id);
                    let is_done = completion.is_some();
                    let is_skipped = completion.map(|c| c.skipped).unwrap_or(false);
                    let item_id = item.id.clone();
                    let gid = group.id.clone();
                    let d = date.clone();

                    rsx! {
                        div {
                            style: "padding: 8px 12px; display: flex; align-items: center; gap: 10px; border-bottom: 1px solid #f8f8f8;",

                            if is_done && !is_skipped {
                                // Completed
                                div {
                                    style: "width: 24px; height: 24px; border-radius: 4px; background: #4caf50; display: flex; align-items: center; justify-content: center; color: white; font-size: 14px;",
                                    "✓"
                                }
                                span { style: "flex: 1; font-size: 14px; color: #888; text-decoration: line-through;", "{item.name}" }
                            } else if is_skipped {
                                // Skipped
                                div {
                                    style: "width: 24px; height: 24px; border-radius: 4px; background: #ccc; display: flex; align-items: center; justify-content: center; color: white; font-size: 14px;",
                                    "—"
                                }
                                span { style: "flex: 1; font-size: 14px; color: #aaa;", "{item.name}" }
                            } else {
                                // Not done — interactive
                                button {
                                    style: "width: 24px; height: 24px; border-radius: 4px; border: 2px solid #ddd; background: white; cursor: pointer;",
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
                                span { style: "flex: 1; font-size: 14px;", "{item.name}" }
                                button {
                                    style: "padding: 4px 8px; background: none; border: 1px solid #ddd; border-radius: 4px; font-size: 12px; color: #888; cursor: pointer;",
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
                                    "Skip"
                                }
                            }

                            if item.estimated_duration_min > 0 {
                                span {
                                    style: "font-size: 11px; color: #aaa;",
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

// --- Group List View (Task 6.1) ---

#[component]
fn GroupListView(
    groups: Vec<RoutineGroup>,
    on_add: EventHandler<()>,
    on_select: EventHandler<String>,
    on_back: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px;",
                    button {
                        style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                        onclick: move |_| on_back.call(()),
                        "Back"
                    }
                    h1 {
                        style: "font-size: 24px; font-weight: 600; margin: 0; color: #1a1a2e;",
                        "Routine Groups"
                    }
                }
                button {
                    style: "padding: 8px 16px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_add.call(()),
                    "+ Add Group"
                }
            }

            if groups.is_empty() {
                div {
                    style: "text-align: center; padding: 48px 16px; color: #888;",
                    p { "No routine groups. Create one to get started." }
                }
            } else {
                for group in &groups {
                    {
                        let id = group.id.clone();
                        rsx! {
                            div {
                                style: "padding: 12px; border-bottom: 1px solid #eee; cursor: pointer; display: flex; justify-content: space-between; align-items: center;",
                                onclick: move |_| on_select.call(id.clone()),
                                div {
                                    span { style: "font-size: 15px; font-weight: 500;", "{group.name}" }
                                }
                                div {
                                    style: "display: flex; gap: 6px;",
                                    span {
                                        style: "background: #e8eef4; color: #4a6fa5; padding: 2px 8px; border-radius: 4px; font-size: 12px;",
                                        "{group.frequency}"
                                    }
                                    span {
                                        style: "background: #f0f0e8; padding: 2px 8px; border-radius: 4px; font-size: 12px;",
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

// --- Add Group View (Task 6.1) ---

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
        div {
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                h2 { style: "font-size: 18px; font-weight: 600; margin: 0;", "New Group" }
                button {
                    style: "padding: 8px 16px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
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

            if let Some(err) = &*save_error.read() {
                div {
                    style: "background: #fee; color: #c33; padding: 8px 12px; border-radius: 6px; margin-bottom: 12px; font-size: 14px;",
                    "Save failed: {err}"
                }
            }

            div {
                style: "display: flex; flex-direction: column; gap: 16px;",

                // Name
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Name" }
                    input {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; box-sizing: border-box;",
                        r#type: "text",
                        placeholder: "e.g. Morning Routine",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }

                // Frequency
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Frequency" }
                    select {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; background: white;",
                        value: "{frequency}",
                        onchange: move |e| frequency.set(e.value()),
                        option { value: "daily", "Daily" }
                        option { value: "weekly", "Weekly" }
                        option { value: "custom", "Custom" }
                    }
                }

                // Time of day
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Time of Day" }
                    select {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; background: white;",
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

// --- Edit Group View (Task 6.4) ---

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
        div {
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
                h2 { style: "font-size: 18px; font-weight: 600; margin: 0;", "Edit Group" }
                button {
                    style: "padding: 8px 16px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    disabled: *saving.read(),
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

            div {
                style: "display: flex; flex-direction: column; gap: 16px;",
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Name" }
                    input {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; box-sizing: border-box;",
                        r#type: "text",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Frequency" }
                    select {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; background: white;",
                        value: "{frequency}",
                        onchange: move |e| frequency.set(e.value()),
                        option { value: "daily", "Daily" }
                        option { value: "weekly", "Weekly" }
                        option { value: "custom", "Custom" }
                    }
                }
                div {
                    label { style: "font-size: 13px; font-weight: 600; color: #888; display: block; margin-bottom: 4px;", "Time of Day" }
                    select {
                        style: "width: 100%; padding: 10px 12px; border: 1px solid #ddd; border-radius: 6px; font-size: 15px; background: white;",
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

// --- Group Detail View (Task 6.2 + 6.5) ---

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
        div {
            // Header
            div {
                style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px;",
                    button {
                        style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                        onclick: move |_| on_back.call(()),
                        "Back"
                    }
                    h1 {
                        style: "font-size: 20px; font-weight: 600; margin: 0; color: #1a1a2e;",
                        "{group_name}"
                    }
                }
                button {
                    style: "padding: 8px 12px; background: none; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 14px;",
                    onclick: move |_| on_edit.call(gid_for_edit.clone()),
                    "Edit"
                }
            }

            // Items list
            h3 {
                style: "font-size: 13px; font-weight: 600; color: #888; text-transform: uppercase; letter-spacing: 0.5px; margin: 0 0 8px 0;",
                "Items"
            }

            if items.read().is_empty() {
                p { style: "color: #888; font-size: 14px; margin-bottom: 12px;", "No items yet" }
            } else {
                for item in items.read().iter() {
                    div {
                        style: "padding: 8px 12px; border-bottom: 1px solid #f0f0f0; display: flex; justify-content: space-between; align-items: center;",
                        span { style: "font-size: 14px;", "{item.name}" }
                        span { style: "font-size: 12px; color: #888;", "{item.estimated_duration_min}m" }
                    }
                }
            }

            // Add item form
            div {
                style: "margin-top: 12px; display: flex; gap: 8px; align-items: center;",
                input {
                    style: "flex: 1; padding: 8px 10px; border: 1px solid #ddd; border-radius: 6px; font-size: 14px;",
                    r#type: "text",
                    placeholder: "New item name",
                    value: "{new_item_name}",
                    oninput: move |e| new_item_name.set(e.value()),
                }
                input {
                    style: "width: 50px; padding: 8px 6px; border: 1px solid #ddd; border-radius: 6px; font-size: 14px; text-align: center;",
                    r#type: "number",
                    placeholder: "min",
                    value: "{new_item_duration}",
                    oninput: move |e| new_item_duration.set(e.value()),
                }
                button {
                    style: "padding: 8px 12px; background: #4a6fa5; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px;",
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

            // 7-day history grid (Task 6.5)
            if !items.read().is_empty() {
                HistoryGrid { items: items.read().clone(), history: history.read().clone() }
            }
        }
    }
}

// --- History Grid (Task 6.5) ---

#[component]
fn HistoryGrid(items: Vec<RoutineItem>, history: Vec<CompletionEntry>) -> Element {
    let today = chrono::Utc::now().date_naive();
    let days: Vec<String> = (0..7)
        .rev()
        .map(|i| {
            (today - chrono::Duration::days(i))
                .format("%Y-%m-%d")
                .to_string()
        })
        .collect();

    let day_labels: Vec<String> = (0..7)
        .rev()
        .map(|i| {
            (today - chrono::Duration::days(i))
                .format("%a")
                .to_string()
        })
        .collect();

    rsx! {
        div {
            style: "margin-top: 24px;",
            h3 {
                style: "font-size: 13px; font-weight: 600; color: #888; text-transform: uppercase; letter-spacing: 0.5px; margin: 0 0 8px 0;",
                "7-Day History"
            }

            // Header row: day labels
            div {
                style: "display: grid; grid-template-columns: 120px repeat(7, 1fr); gap: 2px; font-size: 11px; color: #888; margin-bottom: 4px;",
                div {}
                for label in &day_labels {
                    div { style: "text-align: center;", "{label}" }
                }
            }

            // Item rows
            for item in &items {
                div {
                    style: "display: grid; grid-template-columns: 120px repeat(7, 1fr); gap: 2px; margin-bottom: 2px;",
                    div {
                        style: "font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; padding: 4px 0;",
                        "{item.name}"
                    }
                    for day in &days {
                        {
                            let completion = history.iter().find(|c| c.item_id == item.id && c.date == *day);
                            let (bg, label) = match completion {
                                Some(c) if c.skipped => ("#e0e0e0", "—"),
                                Some(_) => ("#4caf50", "✓"),
                                None => ("#f5f5f5", ""),
                            };
                            rsx! {
                                div {
                                    style: "background: {bg}; border-radius: 3px; height: 24px; display: flex; align-items: center; justify-content: center; font-size: 11px; color: white;",
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
