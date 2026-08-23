//! Fixed client copy: what every Host-backed screen must say, and what it says
//! where the Host has said nothing.
//!
//! These strings are the client's own standing claims about its lack of
//! authority, so they live in one place and are read the same way by every
//! screen. A screen imports its banner; it never re-authors one, softens one,
//! or invents a second placeholder for absence.

/// Placeholder for every value the Host has not stated.
pub const ABSENT: &str = "—";

/// Fixed copy. Rendered before any Host content and never softened.
pub const RECALL_POLICY_DISCLOSURE: &str = "NO AUTHORITY · THIS CLIENT IS NOT MEMORY, IDENTITY, OR POLICY AUTHORITY · NO STATE APPEARS WITHOUT AN AUTHENTICATED ATHANOR HOST SNAPSHOT · HOUSE, ROOM, SPIRIT, AND SESSION COME ONLY FROM THE HOST SNAPSHOT, NEVER FROM SHELL CONTEXT OR THE WORKING DIRECTORY";

/// Build-only: the dispatch screen authors a packet and never spawns an agent.
pub const DISPATCH_DISCLOSURE: &str = "BUILD ONLY · THIS SCREEN BUILDS A BOUNDED OMP TASK PACKET · IT NEVER SPAWNS OR EXECUTES AN AGENT · THE AUTHENTICATED HOST VALIDATES THE REQUEST AND RETURNS A READY OR REJECTED PACKET";

/// Read-only: the spellbook is the Host's, never a path this client opens.
pub const FAMILIAR_STATUS_DISCLOSURE: &str = "READ ONLY · FAMILIAR STATUS COMES FROM THE AUTHENTICATED ATHANOR HOST · THE CLIENT DOES NOT READ A SPELLBOOK PATH, INFER A ROOM, DISPATCH, SPAWN, OR EXECUTE AN AGENT";

/// Observation only: separate channels, never one collapsed verdict.
pub const HEALTH_DISCLOSURE: &str = "OBSERVATION ONLY · EACH CHANNEL REPORTS ITS OWN REAL HOST EVENT · TRANSPORT, BINDING, RECALL HEALTH, PAPER BOAT DELIVERY, AND PROTOCOL REFUSAL ARE NEVER COLLAPSED INTO ONE VERDICT";

/// Read-only: lane availability is the Host's word, never the shell's.
pub const ROUTING_DISCLOSURE: &str = "READ ONLY · WORKER LANES AND ADVISOR COME FROM THE AUTHENTICATED ATHANOR HOST · THIS SCREEN DOES NOT DISPATCH, START AGENTS, OR INFER AVAILABILITY FROM THE SHELL";
