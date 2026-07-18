// Single source of truth: events.json (shared with Rust build.rs)
// When updating event names, edit events.json in this directory.
import eventsJson from "./events.json";

export const EVENTS = eventsJson as typeof eventsJson;
