use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        h1 {"2026-03-24"}
        h2 {"What happened today? (Add as much detail as you want)"}
        div {
            "I woke up earlier than usual..."
        }
    }
}
