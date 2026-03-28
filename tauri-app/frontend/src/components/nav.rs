use dioxus::prelude::*;

use crate::Tab;

#[component]
pub fn BottomNav(active: Tab, on_switch: EventHandler<Tab>) -> Element {
    let tab_style = |tab: Tab| -> String {
        let is_active = active == tab;
        let bg = if is_active { "#16213e" } else { "transparent" };
        let color = if is_active { "#e94560" } else { "#8892a4" };
        let font_weight = if is_active { "600" } else { "400" };

        format!(
            "
            flex: 1;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 48px;
            padding: 8px 0;
            border: none;
            background: {bg};
            color: {color};
            font-size: 14px;
            font-weight: {font_weight};
            font-family: inherit;
            cursor: pointer;
            border-radius: 8px;
            transition: background 0.15s, color 0.15s;
            -webkit-tap-highlight-color: transparent;
            "
        )
    };

    rsx! {
        nav {
            style: "
                display: flex;
                align-items: center;
                gap: 4px;
                padding: 4px 8px;
                background: #1a1a2e;
                border-top: 1px solid #16213e;
                position: fixed;
                bottom: 0;
                left: 0;
                right: 0;
                z-index: 100;
            ",

            button {
                style: "{tab_style(Tab::Journal)}",
                onclick: move |_| on_switch.call(Tab::Journal),
                "Journal"
            }

            button {
                style: "{tab_style(Tab::Routines)}",
                onclick: move |_| on_switch.call(Tab::Routines),
                "Routines"
            }

            button {
                style: "{tab_style(Tab::Settings)}",
                onclick: move |_| on_switch.call(Tab::Settings),
                "Settings"
            }
        }
    }
}
