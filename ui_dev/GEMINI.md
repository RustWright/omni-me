# Omni-Me Gemini Workspace Context

This document provides essential context for working on the Omni-Me project. It outlines the project's architecture, key technologies, and development workflows to help guide AI-assisted development.

## Project Overview

Omni-Me is an offline-first, privacy-focused personal management application designed to run on Android via Tauri v2. It aims to integrate various aspects of personal productivity, including note-taking, task and goal management, routine tracking, and more, into a single, cohesive system.

The core philosophy is built on an **event sourcing** model, where all changes are stored as immutable, append-only events. This provides a conflict-free sync strategy between the local device and a backend server, ensuring data integrity and a complete history of all operations.

The application leverages a Rust-based monorepo with a clear separation of concerns:
-   **`core`**: A shared library containing the primary business logic, database interactions, event sourcing implementation, and LLM integration.
-   **`server`**: An Axum-based web server that manages data sync and exposes an API for the client.
-   **`tauri-app`**: A Tauri v2 desktop application.
    -   **`src-tauri`**: The Rust backend of the Tauri application.
    -   **`frontend`**: The user interface, built with the Dioxus framework (Rust compiled to WASM).

### Key Technologies

-   **Backend & Core Logic**: Rust
-   **Application Framework**: Tauri v2 (targeting Android)
-   **UI Framework**: Dioxus 0.7 (WASM)
-   **Database**: Embedded SurrealDB (single-file `kv-surrealkv` engine)
-   **Sync Strategy**: Custom Event Sourcing
-   **LLM Integration**: Gemini Flash API (via a `LlmClient` trait)

## Building and Running

The project is managed as a Rust workspace. Build and test commands should be run from the project root.

### Building

**Backend Crates (Core & Server)**
To build the backend components in release mode:
```bash
cargo build --release -p omni-me-core -p omni-me-server
```

**Frontend (Dioxus WASM)**
The frontend requires the `dioxus-cli` to be installed (`cargo install dioxus-cli`). To build the frontend assets:
```bash
cd tauri-app/frontend
dx build --platform web --release
```

### Testing

Run tests for the `core` and `server` crates:
```bash
cargo test -p omni-me-core -p omni-me-server
```
There are currently no automated tests for the `frontend` or `tauri-app` crates.

### Running the Application

**1. Run the Sync Server:**
In a dedicated terminal, start the backend server:
```bash
cargo run -p omni-me-server
```

**2. Run the Tauri Application:**
In another terminal, run the Tauri development server:
```bash
# TODO: Confirm the exact command, it's likely one of the following
# cd tauri-app && cargo tauri dev
```
*(Self-correction: The specific `cargo tauri` command for running the dev environment with an Android target needs to be confirmed, but `cargo tauri dev` is the standard starting point.)*

## Development Conventions

-   **Event Sourcing**: All state changes must be modeled as events. The current state is derived from replaying these events into "projections." This is fundamental to the sync strategy. See `core/src/events/` for existing implementations.
-   **Immutable Data**: Events are immutable facts. Raw user input (like a note) should never be modified directly. Instead, new events are created to represent changes or derivations (e.g., `note_updated`, `note_llm_processed`).
-   **LLM Abstraction**: All interactions with language models must go through the `LlmClient` trait defined in `core/src/llm/client.rs`. This allows for swapping providers in the future.
-   **Configuration**: The project uses a layered configuration approach. Be mindful of where different settings are stored (e.g., `tauri.conf.json` for the app, environment variables for the server).
-   **CI/CD**: The CI pipeline is defined in `.github/workflows/ci.yml`. It's the source of truth for build and test procedures. All changes should pass the checks defined there.
