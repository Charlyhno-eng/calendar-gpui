# Agents.md --- Calendar-GPUI

## Project Overview

Calendar-GPUI is a desktop calendar application built in **Rust** with
**GPUI**. Its goal is to provide a fast, polished calendar experience
inspired by Google Calendar, while making calendar data and actions
accessible to AI agents through **MCP (Model Context Protocol)**.

## Core Features

-   **Calendar views:** Open in week view by default, with an option to
    switch to month view.
-   **Timed tasks and events:** Create, edit, and remove tasks or events
    at specific dates and times.
-   **Links and meetings:** Attach URLs to events, including video-call
    links such as Google Meet.
-   **Weekends and holidays:** Visually distinguish weekends and display
    public holidays.
-   **MCP integration:** Expose suitable calendar operations and event
    data to MCP-compatible AI agents, with clear validation and safe
    handling of changes.
-   **Reliable date and time handling:** Account for time zones and
    event boundaries consistently.

## Design Direction

The interface should feel modern, focused, and desktop-native.

-   Use a **predominantly black or near-black background**.
-   Add subtle **violet/purple tones** for accents, selected dates,
    focus states, and highlights.
-   Use restrained transparency and layered surfaces to create a
    slightly translucent look without harming readability.
-   Keep text contrast high and visual hierarchy clear. Events should be
    easy to scan, and weekends, holidays, and the current time should be
    distinguishable at a glance.
-   Prefer subtle borders, soft highlights, and restrained animation
    over heavy decoration.

## Technical Guidelines

-   Use idiomatic, maintainable **Rust** and follow the project's
    formatting and linting conventions.
-   Run the relevant formatter, linter, and tests when making changes
    (for example, `cargo fmt`, `cargo clippy`, and `cargo test`, as
    supported by the repository).
-   Follow GPUI's established patterns for views, state, event handling,
    and rendering. Keep UI logic separated from calendar and integration
    logic where practical.
-   Validate user input and MCP requests. Handle errors explicitly; do
    not silently discard failed operations.
-   Keep date/time logic testable, and avoid duplicating business rules
    between the UI and MCP layer.
-   Prefer small, focused changes and add tests for important behavior
    or bug fixes.

## Expected Agent Behavior

When working on this repository, an AI coding agent should:

1.  Inspect the existing codebase and conventions before making
    architectural changes.
2.  Preserve the project's Rust/GPUI approach and avoid introducing
    dependencies without a clear need.
3.  Prioritize correctness, accessibility, and readable code over
    unnecessary complexity.
4.  Keep MCP tools narrowly scoped, documented, and safe to call;
    confirm destructive or ambiguous operations when appropriate.
5.  Update relevant tests and documentation when behavior changes.
6.  Summarize changes and mention any checks that could not be run.

## Initial Priorities

Build a dependable calendar foundation first: event and task models,
date/time handling, week and month navigation, event creation and
editing, and clear visual treatment for weekends and holidays. Then
connect the same core operations to MCP so the UI and AI agents interact
with consistent calendar data.
