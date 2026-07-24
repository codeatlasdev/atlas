# Atlas Memory Architecture

> Cognitive Memory Engine — Design Document v1.0
> Date: 2026-07-14
> Status: DESIGN (not implemented)

---

## 1. Problem Statement

### 1.1 What We Need to Solve

Atlas is a native macOS daemon that manages servers, deploys services, and orchestrates AI agents. The daemon runs continuously as a launchd agent, maintaining SSH connections and routing AI requests. It needs a **persistent, structured memory system** that enables:

1. **Agent Memory (Local, Multi-Session)**: AI agents must retain knowledge across sessions — user preferences, project conventions, past decisions, correction history. Without this, every session starts from zero.

2. **Cross-Agent Knowledge Sharing**: Multiple agents (AI chat, server monitor, deploy agent) operate on the same codebase/infrastructure. Knowledge discovered by one agent must be accessible to others without explicit communication.

3. **Temporal Awareness**: Facts change. A server was running Ubuntu 22.04 last month but was upgraded to 24.04 today. The memory must know *when* something was true, not just *that* it was true.

4. **Context Assembly**: Before each LLM call, the daemon must assemble the most relevant context from potentially thousands of stored memories — selecting what matters for *this specific query* in sub-10ms.

### 1.2 Constraints

| Constraint | Requirement | Rationale |
|-----------|-------------|-----------|
| Platform | macOS 14+ / Apple Silicon | Atlas is a native macOS app |
| Architecture | Local-first, single-file | No external services, no network dependencies |
| Language | Rust (tokio async runtime) | Daemon is Rust; memory must be in-process |
| Latency | < 10ms for retrieval (System 1) | Context assembly happens on every LLM call |
| Concurrency | Single writer, multiple readers | One daemon process, multiple async tasks |
| Storage | ~/.atlas/memory.db (single file) | Simple deployment, easy backup |
| Privacy | Zero telemetry, zero cloud | All data stays on the developer's machine |
| Scale | 10K-100K memories, 1K-10K entities | Single developer, multiple projects |

### 1.3 Non-Goals (Explicit)

- Distributed consensus or multi-node operation
- Cloud sync or team collaboration (future, not v1)
- Real-time streaming to external services
- Supporting non-macOS platforms in v1

---

## 2. State of the Art Analysis

### 2.1 Comparative Analysis

| System | Storage | Retrieval | Temporal | Forgetting | Local-first | Graph | Latency |
|--------|---------|-----------|----------|------------|-------------|-------|---------|
| **Claude Code** | Markdown files | Full injection | ❌ | Dreaming (2026) | ✅ | ❌ | N/A (no search) |
| **Cursor AI** | Vector index | Hybrid (BM25+vec) | ❌ | ❌ | ✅ | ❌ | ~50ms |
| **Mem0** | Vector + Graph | BM25+Vec+Entity | Partial | ❌ | ❌ (cloud) | ✅ | ~100ms |
| **Zep/Graphiti** | Neo4j | Multi-hop temporal | ✅✅ | Supersession | ❌ (Neo4j) | ✅✅ | ~200ms |
| **CrewAI v1.15** | LanceDB | Hybrid + cognitive | ❌ | ✅ (curves) | ✅ | ❌ | ~30ms |
| **Cortex** | SQLite | Event-sourced | Partial | Decay | ✅ | ✅ | <1ms |
| **ContextDB** | Custom (MVCC) | Graph+Vec+Rel | ❌ | ❌ | ✅ | ✅ | <5ms |
| **Augment Code** | Cloud graph | Dependency-aware | ❌ | ❌ | ❌ | ✅✅ | ~300ms |
| **LangGraph** | Postgres/SQLite | By thread_id | Checkpoints | TTL | Partial | ❌ | ~10ms |
| **Hindsight** | PostgreSQL | 4-network parallel | ✅ | Confidence decay | ❌ | ✅✅ | ~150ms |

**Sources**: Anthropic Claude Code docs (2026), Cursor architecture blog, Mem0 benchmarks (LongMemEval +26pts), Zep/Graphiti paper (arXiv:2501.13956), CrewAI v1.15 changelog, Cortex GitHub (217 commits), ContextDB v1.0.0 release, Augment Context Engine MCP docs, LangGraph persistence docs, Hindsight ACL 2026 demo.

### 2.2 Identified Gaps

1. **No system combines bitemporal awareness + local-first + graph + vector**: Graphiti has temporality but requires Neo4j. Local systems (Cortex, ContextDB) lack rigorous bitemporality.

2. **No coding tool implements cognitive retrieval**: All use recency or similarity. None implement ACT-R activation-based retrieval where context *spreads activation* to related memories.

3. **No system does background consolidation locally**: Claude Code's Dreaming is cloud-only. No local daemon does inter-session memory refinement.

4. **No multi-agent coordination without explicit messaging**: All multi-agent systems (CrewAI, LangGraph) require explicit communication. None use stigmergic (pheromone-based) indirect coordination.

5. **No system explains past decisions via replay**: Event sourcing exists (ESAA) but no coding tool lets you ask "why did the agent do X last Tuesday?" and get a deterministic trace.

### 2.3 Innovation Opportunity

Atlas can be the **first local-first cognitive memory engine** that combines:
- Bitemporal facts (know when things were true)
- Dual-process retrieval (fast path + background reasoning)
- Stigmergic multi-agent coordination (zero-communication)
- Deterministic replay (explainable agent behavior)
- Single-file, zero-network, pure Rust

No existing system — academic or commercial — combines all five.

---

## 3. Proposed Architecture

### 3.1 Layer Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        ATLAS DAEMON (tokio)                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌───────────────────────────────────────────────────────────────┐   │
│  │                    CONTEXT ASSEMBLER                            │   │
│  │  Selects memories for LLM context window (page-in/page-out)   │   │
│  │  Inspired by: MemGPT (arXiv:2310.08560)                      │   │
│  └───────────────────────────┬───────────────────────────────────┘   │
│                              │                                        │
│  ┌───────────────┐    ┌─────▼─────────────┐                         │
│  │  SYSTEM 1     │    │    SYSTEM 2        │                         │
│  │  (Fast Path)  │    │    (Slow Path)     │                         │
│  │               │    │                    │                         │
│  │ • Index lookup│    │ • Consolidation    │                         │
│  │ • Activation  │    │ • Contradiction    │                         │
│  │   scoring     │    │   detection        │                         │
│  │ • KG traverse │    │ • Decay curves     │                         │
│  │ • Pheromone   │    │ • Pattern extract  │                         │
│  │   read        │    │ • Entity merging   │                         │
│  │               │    │                    │                         │
│  │ Budget: <10ms │    │ Runs: idle CPU     │                         │
│  └───────┬───────┘    └────────┬───────────┘                         │
│          │                     │                                      │
│  ┌───────▼─────────────────────▼───────────────────────────────┐     │
│  │              MEMORY HIERARCHY (4 Layers)                     │     │
│  │                                                              │     │
│  │  ┌─────────────────────────────────────────────────────┐    │     │
│  │  │ KNOWLEDGE — Permanent, versioned (supersession)      │    │     │
│  │  │ Facts, configs, canonical decisions                  │    │     │
│  │  └─────────────────────────────────────────────────────┘    │     │
│  │  ┌─────────────────────────────────────────────────────┐    │     │
│  │  │ WISDOM — Evidence-gated revision                     │    │     │
│  │  │ Patterns, heuristics, learned strategies             │    │     │
│  │  └─────────────────────────────────────────────────────┘    │     │
│  │  ┌─────────────────────────────────────────────────────┐    │     │
│  │  │ MEMORY — Ebbinghaus decay (reinforced by access)     │    │     │
│  │  │ Episodes, observations, session summaries            │    │     │
│  │  └─────────────────────────────────────────────────────┘    │     │
│  │  ┌─────────────────────────────────────────────────────┐    │     │
│  │  │ INTELLIGENCE — Ephemeral (TTL, session-scoped)       │    │     │
│  │  │ Working context, scratchpads, intermediate results   │    │     │
│  │  └─────────────────────────────────────────────────────┘    │     │
│  └──────────────────────────────────────────────────────────────┘     │
│                              │                                        │
│  ┌───────────────────────────▼──────────────────────────────────┐    │
│  │                    STORAGE ENGINE                              │    │
│  │                                                               │    │
│  │  Primary: redb (pure Rust, ACID, single-file, CoW B+ trees) │    │
│  │  Vector:  shodh-redb (IVF-PQ, 95% recall@1, 1000+ QPS)     │    │
│  │  Text:    tantivy (BM25 keyword search, Rust-native)         │    │
│  │  Events:  Append-only table (hash-chained, replayable)       │    │
│  │                                                               │    │
│  │  File: ~/.atlas/memory.db                                    │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │              EVENT LOG (Source of Truth)                       │    │
│  │  • Append-only, content-addressed (SHA-256)                  │    │
│  │  • Every mutation is an event                                │    │
│  │  • Projections derive all other state                        │    │
│  │  • Deterministic replay from any point                       │    │
│  └──────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Storage Engine Choice

**Decision: `redb` + `shodh-redb`**

| Criterion | redb | SQLite | RocksDB | SurrealDB |
|-----------|------|--------|---------|-----------|
| Pure Rust | ✅ | ❌ (C via FFI) | ❌ (C++) | ✅ |
| Single file | ✅ | ✅ | ❌ (directory) | ✅ |
| ACID | ✅ | ✅ | ❌ (eventual) | ✅ |
| Concurrency | MVCC (CoW) | WAL | LSM | MVCC |
| Vector search | via shodh-redb | ❌ (manual) | ❌ | ✅ |
| Binary size | ~200KB | ~1.5MB | ~15MB | ~50MB |
| Crash safety | CoW (never corrupt) | WAL (good) | WAL | CoW |
| License | MIT | Public Domain | Apache/GPL | BSL |

**Justification**:
- **Pure Rust**: No C/C++ FFI means simpler builds, no linker issues, deterministic compilation on Apple Silicon.
- **CoW B+ Trees**: Readers never block writers. Perfect for async daemon where System 2 writes while System 1 reads.
- **shodh-redb extends redb**: Same file, adds IVF-PQ vector search + TTL tables + CDC. Benchmarked at 95% recall@1 on SIFT1M, 1000+ QPS single-threaded (source: shodh-redb README).
- **Single file**: `~/.atlas/memory.db` — easy to backup, migrate, delete.

**References**: redb (github.com/cberner/redb, 1580+ commits), shodh-redb benchmarks (lib.rs/crates/shodh-redb).

### 3.3 Memory Hierarchy Design

Based on "The Missing Knowledge Layer" (arXiv:2604.11364) and CoALA (arXiv:2309.02427):

| Layer | Persistence | Update Semantics | Decay | Example |
|-------|-------------|-----------------|-------|---------|
| **Knowledge** | Indefinite | Supersession (old version preserved, new version active) | Never | "Server X runs Ubuntu 24.04" |
| **Wisdom** | Indefinite | Evidence-gated (needs N confirmations to revise) | Never | "Always run migrations before deploy" |
| **Memory** | Decaying | Ebbinghaus curve (base_level = ln(n)/d^0.5) | Yes, logarithmic | "User asked about Redis config yesterday" |
| **Intelligence** | TTL (session) | Overwrite | Auto-expire | "Current working branch is feature/auth" |

**Decay formula** (adapted from ACT-R, Anderson & Lebiere 1998):

```
activation(i) = base_level(i) + spreading_activation(i)

base_level(i) = ln(Σ t_j^(-d))
  where t_j = time since j-th access, d = 0.5 (decay rate)

spreading_activation(i) = Σ (w_k * S_ki)
  where w_k = weight of context element k
        S_ki = association strength between k and memory i
```

### 3.4 Graph Model

**Nodes** (entities):

```rust
struct Entity {
    id: Ulid,
    name: String,
    entity_type: EntityType, // Server, Service, File, Function, Person, Concept
    summary: String,         // LLM-generated, updated on observe
    properties: BTreeMap<String, Value>,
    first_observed: DateTime<Utc>,
    last_observed: DateTime<Utc>,
    observation_count: u32,
}
```

**Edges** (relationships):

```rust
struct Relationship {
    id: Ulid,
    source: Ulid,           // Entity ID
    target: Ulid,           // Entity ID
    rel_type: RelationType, // DependsOn, Calls, Manages, DeployedOn, DecidedBecause
    weight: f32,            // 0.0-1.0, decays over time
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>, // None = still valid
    recorded_at: DateTime<Utc>,         // When system learned this
    evidence: Vec<Ulid>,    // Event IDs that support this relationship
}
```

**Pheromone Map** (stigmergy):

```rust
struct Pheromone {
    key: String,            // e.g., "file:src/main.rs" or "concept:auth"
    strength: f32,          // 0.0-1.0, decays exponentially
    source_agent: String,   // Which agent deposited
    deposited_at: DateTime<Utc>,
    decay_rate: f32,        // Per-hour multiplier (default 0.9)
}
```

### 3.5 Vector Index Strategy

**Model**: AllMiniLM-L6-v2 via `fastembed-rs` (384 dimensions, ONNX Runtime)
**Index**: IVF-PQ via `shodh-redb` (in-file, no external process)
**Quantization**: f16 for storage, f32 for computation

**What gets embedded**:
- Memory content (all layers)
- Entity summaries
- Relationship descriptions
- Session summaries

**What does NOT get embedded** (structured lookup instead):
- Pheromone keys (exact-match)
- Entity names (ART prefix tree)
- Temporal queries (B+ tree range scan)

### 3.6 Temporal Model

**Bitemporal** (inspired by Graphiti, arXiv:2501.13956):

```
valid_time:     When the fact was TRUE in the real world
recorded_time:  When the system LEARNED about the fact
```

Every fact, relationship, and observation carries both timestamps. This enables:

- `as_of(valid_time)`: "What was true about server X on March 1st?"
- `as_known(recorded_time)`: "What did the agent believe when it made that decision?"
- `supersession`: New fact doesn't delete old — marks it `valid_until = now`

**Implementation**: Append-only writes with validity windows. Queries default to `valid_until IS NULL` (current truth). Time-travel queries specify explicit time bounds.

---

## 4. Data Model

### 4.1 Complete Schema

```rust
// ═══════════════════════════════════════════════════════════
// EVENT LOG (Source of Truth — append-only)
// ═══════════════════════════════════════════════════════════

struct Event {
    id: Ulid,                    // Monotonic, sortable
    session_id: Ulid,
    timestamp: DateTime<Utc>,
    event_type: EventType,
    payload: Vec<u8>,            // MessagePack-encoded
    content_hash: [u8; 32],      // SHA-256 of payload
    parent_event: Option<Ulid>,  // Causal chain
}

enum EventType {
    // Agent actions
    SessionStart,
    SessionEnd,
    QuerySubmitted,
    ResponseGenerated,
    ToolInvoked,
    ToolResult,
    
    // Memory mutations
    MemoryCreated,
    MemoryAccessed,
    MemoryUpdated,
    MemorySuperseded,
    MemoryDecayed,
    
    // Knowledge graph mutations
    EntityCreated,
    EntityUpdated,
    RelationshipCreated,
    RelationshipInvalidated,
    
    // System
    ConsolidationRun,
    PheromoneDeposited,
    PheromoneDecayed,
}

// ═══════════════════════════════════════════════════════════
// MEMORY STORE (Projected from events)
// ═══════════════════════════════════════════════════════════

struct Memory {
    id: Ulid,
    layer: MemoryLayer,          // Knowledge, Wisdom, Memory, Intelligence
    content: String,             // Human-readable content
    memory_type: MemoryType,     // Fact, Decision, Preference, Convention, Observation
    
    // Temporal (bitemporal)
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    recorded_at: DateTime<Utc>,
    
    // Retrieval scoring
    importance: f32,             // 0.0-1.0, set at creation
    activation: f32,            // ACT-R activation level (computed)
    access_count: u32,
    last_accessed: DateTime<Utc>,
    
    // Relations
    superseded_by: Option<Ulid>,
    evidence_events: Vec<Ulid>,
    linked_entities: Vec<Ulid>,
    
    // Search
    embedding: Vec<f32>,         // 384-dim (AllMiniLM-L6-v2)
    keywords: Vec<String>,       // Extracted for BM25
    
    // Provenance
    source_session: Ulid,
    source_agent: String,
}

enum MemoryLayer {
    Knowledge,    // Permanent, versioned
    Wisdom,       // Evidence-gated
    Memory,       // Decaying
    Intelligence, // Ephemeral (TTL)
}

enum MemoryType {
    Fact,         // "Server X has 16GB RAM"
    Decision,     // "We chose PostgreSQL for the auth service"
    Preference,   // "User prefers spaces over tabs"
    Convention,   // "All services use port 8080"
    Observation,  // "Deploy failed because of disk space"
    Correction,   // "Actually, it's port 3000 not 8080"
}

// ═══════════════════════════════════════════════════════════
// KNOWLEDGE GRAPH (Projected from events)
// ═══════════════════════════════════════════════════════════

struct Entity {
    id: Ulid,
    name: String,
    entity_type: EntityType,
    summary: String,
    properties: BTreeMap<String, Value>,
    first_observed: DateTime<Utc>,
    last_observed: DateTime<Utc>,
    observation_count: u32,
    embedding: Vec<f32>,
}

enum EntityType {
    Server,
    Service,
    File,
    Function,
    Module,
    Person,
    Concept,
    Project,
    Technology,
}

struct Relationship {
    id: Ulid,
    source: Ulid,
    target: Ulid,
    rel_type: RelationType,
    weight: f32,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    recorded_at: DateTime<Utc>,
    evidence: Vec<Ulid>,
    properties: BTreeMap<String, Value>,
}

enum RelationType {
    DependsOn,
    Calls,
    Manages,
    DeployedOn,
    PartOf,
    CreatedBy,
    ModifiedBy,
    RelatedTo,
    ConflictsWith,
    Supersedes,
    DecidedBecause,
}

// ═══════════════════════════════════════════════════════════
// PHEROMONE MAP (Stigmergy)
// ═══════════════════════════════════════════════════════════

struct Pheromone {
    key: String,
    strength: f32,
    source_agent: String,
    deposited_at: DateTime<Utc>,
    decay_rate: f32,
    pheromone_type: PheromoneType,
}

enum PheromoneType {
    FileHeat,        // File was recently modified/viewed
    ConflictZone,    // Multiple agents touched same resource
    AttentionTrail,  // Agent spent significant time here
    ErrorSite,       // Errors occurred near this resource
    SuccessMarker,   // Something worked well here
}

// ═══════════════════════════════════════════════════════════
// SESSIONS
// ═══════════════════════════════════════════════════════════

struct Session {
    id: Ulid,
    agent_type: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    summary: Option<String>,       // Generated at session end
    memories_created: Vec<Ulid>,
    entities_touched: Vec<Ulid>,
    token_count: u64,
}
```

### 4.2 Query Examples

**Retrieve relevant context for a query about "nginx configuration":**

```rust
// System 1: parallel multi-signal retrieval (<10ms budget)
async fn retrieve(query: &str, context: &Context) -> Vec<ScoredMemory> {
    let (vec_results, bm25_results, graph_results) = tokio::join!(
        // Signal 1: Vector similarity
        vector_search(embed(query), top_k=20),
        // Signal 2: BM25 keyword match
        keyword_search(extract_keywords(query), top_k=20),
        // Signal 3: Graph neighborhood
        graph_expand(context.active_entities, hops=2, top_k=20),
    );
    
    // Reciprocal Rank Fusion
    let fused = rrf_fusion(vec_results, bm25_results, graph_results, k=60);
    
    // Apply activation scoring (ACT-R)
    let scored = fused.iter().map(|m| {
        let activation = compute_activation(m, context);
        ScoredMemory { memory: m, score: m.rrf_score * activation }
    }).collect();
    
    // Filter by temporal validity
    scored.retain(|m| m.memory.valid_until.is_none());
    
    // Return top-N within token budget
    fit_to_budget(scored, max_tokens=4096)
}
```

**Time-travel query: "What did we know about server X last week?":**

```rust
fn as_of(entity_name: &str, point_in_time: DateTime<Utc>) -> Vec<Memory> {
    memories
        .filter(|m| m.linked_entities.contains(entity_id))
        .filter(|m| m.valid_from <= point_in_time)
        .filter(|m| m.valid_until.map_or(true, |t| t > point_in_time))
        .filter(|m| m.recorded_at <= point_in_time) // as-known filter
        .collect()
}
```

**Graph traversal: "What depends on this failing service?":**

```rust
fn impact_analysis(entity_id: Ulid, depth: u32) -> Vec<(Entity, Vec<Relationship>)> {
    bfs_traverse(
        start: entity_id,
        direction: Incoming,  // Who depends ON this
        rel_types: [DependsOn, Calls, DeployedOn],
        max_depth: depth,
        filter: |r| r.valid_until.is_none(), // Only current relationships
    )
}
```

### 4.3 Index Design

| Data | Index Type | Purpose | Crate |
|------|-----------|---------|-------|
| Memory embeddings | IVF-PQ (384-dim) | Semantic similarity | shodh-redb |
| Memory content | BM25 inverted index | Keyword search | tantivy |
| Entity names | Adaptive Radix Tree | Prefix/exact lookup | blart |
| Relationships (source) | B+ tree | Graph traversal (outgoing) | redb |
| Relationships (target) | B+ tree | Graph traversal (incoming) | redb |
| Events (timestamp) | B+ tree | Time-range queries | redb |
| Pheromones (key) | Hash map | Exact lookup | redb |
| Memory (layer+type) | Composite B+ tree | Filter by category | redb |

---

## 5. Operations

### 5.1 STORE — How Memories Are Created

```
Input Event → Extract → Classify → Embed → Link → Persist
```

**Pipeline**:

1. **Event arrives** (agent action, tool result, user statement)
2. **Extract**: LLM or heuristic extracts salient information
   - Facts: entity-attribute-value triples
   - Decisions: choice + rationale + alternatives considered
   - Observations: what happened + context
3. **Classify**: Determine layer (Knowledge/Wisdom/Memory/Intelligence) and type
   - Heuristic rules first (fast): "user said always → Convention → Knowledge"
   - LLM fallback for ambiguous cases
4. **Embed**: Generate 384-dim embedding via fastembed-rs (AllMiniLM-L6-v2)
5. **Link**: Find related entities and memories
   - Vector search for similar existing memories (A-MEM style auto-linking)
   - Entity extraction → create/update entities in graph
   - Contradiction check against existing Knowledge-layer facts
6. **Persist**: Write event to log, project to memory store + graph

**Deduplication**: Before storing, check if semantically equivalent memory exists (cosine > 0.92). If yes, reinforce existing memory (increment access_count, update last_accessed) instead of creating duplicate.

**Pheromone deposit**: After any store operation, deposit pheromone on touched resources:
```rust
deposit_pheromone(key: "file:{path}", strength: 0.8, type: FileHeat);
```

### 5.2 RETRIEVE — How Memories Are Found

**Dual-process retrieval** (inspired by D-Mem, arXiv:2603.18631):

**System 1 (Fast Path, <10ms)**:
1. Parse query intent (keyword extraction, entity recognition)
2. Parallel retrieval across 3 signals:
   - **Vector**: cosine similarity against memory embeddings
   - **BM25**: keyword matching via tantivy
   - **Graph**: expand from context entities (1-2 hops)
3. Reciprocal Rank Fusion (RRF) to merge results:
   ```
   RRF_score(d) = Σ 1/(k + rank_i(d))  where k=60
   ```
4. Apply ACT-R activation scoring:
   ```
   final_score = rrf_score × activation × temporal_boost
   ```
5. Filter by temporal validity (only current facts)
6. Fit to token budget (greedy by score, respecting diversity)

**System 2 (Slow Path, background)**:
- Triggered when System 1 confidence is low (top score < threshold)
- Multi-hop graph reasoning
- Cross-reference with Wisdom layer
- May trigger LLM for inference over retrieved facts
- Results cached for future System 1 retrieval

**Pheromone influence**: Active pheromones boost retrieval scores:
```rust
if pheromone_strength("file:{related_file}") > 0.5 {
    score *= 1.2; // Recently active resources get priority
}
```

### 5.3 UPDATE — How Memories Evolve

**Decay (Ebbinghaus curve, Memory layer only)**:
```rust
fn decay_pass() {
    for memory in memories.filter(layer == Memory) {
        let hours_since_access = now() - memory.last_accessed;
        let retention = 1.0 / (1.0 + hours_since_access.pow(0.5));
        memory.activation = memory.importance * retention;
        
        if memory.activation < FORGET_THRESHOLD {
            mark_for_review(memory); // System 2 decides final fate
        }
    }
}
```

**Consolidation (Dreaming, inspired by Claude Code Auto Dream)**:
- Trigger: Every 24h or after N=10 sessions (whichever first)
- Phases:
  1. **Deduplicate**: Merge memories with cosine > 0.92
  2. **Resolve contradictions**: If two Knowledge-layer facts conflict, flag for user or use recency
  3. **Promote**: Memory-layer observations that repeated 3+ times → promote to Wisdom
  4. **Summarize**: Compress old episodic memories into concise summaries
  5. **Prune**: Remove Intelligence-layer entries past TTL

**Contradiction resolution**:
```rust
fn resolve_contradiction(existing: &Memory, new: &Memory) {
    if existing.layer == Knowledge && new.layer == Knowledge {
        // Supersession: mark old as superseded, new becomes canonical
        existing.valid_until = Some(now());
        existing.superseded_by = Some(new.id);
        new.evidence_events.push(contradiction_event_id);
    }
}
```

**Evidence accumulation (Wisdom layer)**:
```rust
fn update_wisdom(wisdom: &mut Memory, new_evidence: &Event) {
    wisdom.evidence_events.push(new_evidence.id);
    let evidence_count = wisdom.evidence_events.len();
    
    // Only revise with sufficient evidence (evidence-gated)
    if evidence_count >= REVISION_THRESHOLD {
        // Allow LLM to reformulate the wisdom
        wisdom.content = llm_revise(wisdom, new_evidence);
        wisdom.recorded_at = now();
    }
}
```

### 5.4 FORGET — How Memories Are Removed

**Philosophy**: Memories are never truly deleted — they are invalidated, archived, or decayed below retrieval threshold. This preserves auditability.

**Heat-based eviction** (from redb TTL tables):

```rust
fn eviction_pass() {
    // 1. Intelligence layer: hard TTL
    for mem in memories.filter(layer == Intelligence) {
        if now() - mem.recorded_at > mem.ttl {
            archive(mem); // Move to cold storage, remove from active indexes
        }
    }
    
    // 2. Memory layer: activation threshold
    for mem in memories.filter(layer == Memory) {
        if mem.activation < COLD_THRESHOLD && !mem.has_active_links() {
            archive(mem);
        }
    }
    
    // 3. Pheromones: natural decay
    for pheromone in pheromones.iter_mut() {
        let hours = (now() - pheromone.deposited_at).as_hours();
        pheromone.strength *= pheromone.decay_rate.powf(hours);
        if pheromone.strength < 0.01 {
            remove(pheromone);
        }
    }
}
```

**Archival vs deletion**:
- Archived memories: removed from active indexes (vector, BM25) but kept in event log
- Event log is never pruned (append-only, source of truth)
- Cold storage can be compacted periodically (merge adjacent events)

---

## 6. Integration with Atlas

### 6.1 How the AI Agent Uses Memory

**Pre-response (recall)**:
```
User query arrives → Context Assembler activated
  1. Extract entities/keywords from query
  2. System 1 retrieval (10ms budget)
  3. Assemble context pack:
     - Relevant Knowledge (always included, highest priority)
     - Active Wisdom (if related to query domain)
     - Recent Memory (session continuity)
     - Pheromone alerts (hot zones, conflict warnings)
  4. Pack injected into LLM system prompt
```

**Post-response (Think-in-Memory loop, arXiv:2311.08719)**:
```
LLM response generated → Post-thinking activated
  1. Extract new facts/decisions/observations from response
  2. Store as appropriate memory type/layer
  3. Update entity graph (new entities, new relationships)
  4. Deposit pheromones on touched resources
  5. Check for contradictions with existing knowledge
```

### 6.2 How Agent Sessions Record to Memory

```rust
// On session start
fn on_session_start(agent_type: &str) -> Session {
    let session = Session::new(agent_type);
    emit_event(EventType::SessionStart, &session);
    
    // Load session-relevant context (Intelligence layer)
    let context = recall_for_agent(agent_type);
    session.set_initial_context(context);
    session
}

// On session end
fn on_session_end(session: &Session) {
    // Generate session summary (LLM or heuristic)
    let summary = summarize_session(session);
    
    // Store as Memory-layer observation
    store_memory(Memory {
        layer: MemoryLayer::Memory,
        memory_type: MemoryType::Observation,
        content: summary,
        importance: compute_importance(session),
        ..
    });
    
    emit_event(EventType::SessionEnd, &session);
}
```

### 6.3 Cross-Agent Knowledge Sharing

**Mechanism: Blackboard + Stigmergy** (arXiv:2507.01701 + arXiv:2512.10166)

All agents read/write the same memory store. Coordination emerges from:

1. **Shared Knowledge layer**: Facts stored by any agent are visible to all
2. **Pheromone trails**: Agent A's activity leaves traces that influence Agent B's behavior
3. **Blackboard entries**: Structured observations posted to a shared space

```rust
// Agent A (Deploy Agent) discovers something
store_memory(Memory {
    content: "Service auth-api requires minimum 512MB RAM",
    layer: Knowledge,
    memory_type: Fact,
    source_agent: "deploy",
    ..
});

// Agent B (Monitor Agent), hours later, queries about auth-api
// → automatically retrieves the RAM fact via entity graph traversal
// → no explicit message passing needed
```

**Conflict resolution**: When agents disagree:
- Same layer, same entity → most recent wins (supersession)
- Different confidence → higher confidence wins
- Unresolvable → flag for human review (store as "disputed" in Wisdom layer)

### 6.4 Kanban Integration

The Atlas kanban (task management) integrates as entities in the knowledge graph:

```rust
// Tasks are entities
Entity {
    name: "ATLAS-42: Add SSH key rotation",
    entity_type: EntityType::Task,
    properties: { "status": "in_progress", "priority": "high" },
    ..
}

// Task relationships
Relationship { source: task_id, target: service_id, rel_type: RelatedTo }
Relationship { source: task_id, target: file_id, rel_type: ModifiedBy }
```

**Context enrichment**: When an agent works on a task, the memory system automatically provides:
- Previous decisions related to this task's domain
- Files recently modified in this area (via pheromones)
- Related tasks and their outcomes
- Conventions that apply to this type of change

---

## 7. Implementation Plan

### 7.1 Crate Structure

```
crates/
├── atlas-memory/          # Main memory crate (orchestrates everything)
│   ├── src/
│   │   ├── lib.rs         # Public API
│   │   ├── store.rs       # Storage engine (redb + shodh-redb)
│   │   ├── graph.rs       # Knowledge graph operations
│   │   ├── retrieve.rs    # Multi-signal retrieval + RRF
│   │   ├── embed.rs       # Embedding pipeline (fastembed-rs)
│   │   ├── temporal.rs    # Bitemporal logic
│   │   ├── decay.rs       # Activation scoring + decay curves
│   │   ├── consolidate.rs # System 2 background tasks
│   │   ├── pheromone.rs   # Stigmergic coordination
│   │   ├── event_log.rs   # Append-only event store
│   │   ├── context.rs     # Context assembler (page-in/page-out)
│   │   └── types.rs       # All data types (Memory, Entity, etc.)
│   └── Cargo.toml
```

### 7.2 Dependencies

```toml
[dependencies]
# Storage
redb = "2"                    # Primary KV store (pure Rust, ACID)
shodh-redb = "0.4"           # Vector search extension

# Text search
tantivy = "0.22"             # BM25 keyword search (Rust-native)

# Embeddings
fastembed = "4"              # Local embeddings (AllMiniLM-L6-v2)

# Indexing
blart = "0.2"                # Adaptive Radix Tree for prefix lookups

# Serialization
rmp-serde = "1"              # MessagePack (compact binary, for events)
serde = { version = "1", features = ["derive"] }

# IDs & Time
ulid = "1"                   # Monotonic sortable IDs
chrono = { version = "0.4", features = ["serde"] }

# Async
tokio = { version = "1", features = ["rt", "time"] }

# Hashing
sha2 = "0.10"                # Content-addressing for events
```

### 7.3 Implementation Phases

**Phase 1: Foundation (2 weeks)**
- [ ] Event log (append-only, redb table)
- [ ] Memory CRUD (store, retrieve by ID, list)
- [ ] Basic entity/relationship graph (adjacency in redb)
- [ ] Integration with atlas-daemon (memory as trait behind arc)

**Phase 2: Retrieval (2 weeks)**
- [ ] Embedding pipeline (fastembed-rs, batch embed on store)
- [ ] Vector index (shodh-redb IVF-PQ)
- [ ] BM25 index (tantivy, auto-sync on memory write)
- [ ] RRF fusion (combine 3 signals)
- [ ] Context assembler (fit to token budget)

**Phase 3: Intelligence (2 weeks)**
- [ ] Temporal model (bitemporal queries, supersession)
- [ ] ACT-R activation scoring (decay + spreading activation)
- [ ] Pheromone map (deposit, read, decay loop)
- [ ] System 2 background task (consolidation, contradiction detection)

**Phase 4: Integration (1 week)**
- [ ] Think-in-Memory loop (post-response extraction)
- [ ] Session lifecycle (start/end hooks, summary generation)
- [ ] Cross-agent knowledge (blackboard read/write via daemon)
- [ ] MCP server exposure (optional, for external tools)

### 7.4 Build vs Reuse

| Component | Decision | Rationale |
|-----------|----------|-----------|
| Storage engine | **Reuse** (redb) | Mature, pure Rust, perfect fit |
| Vector search | **Reuse** (shodh-redb) | Same file as redb, benchmarked |
| BM25 search | **Reuse** (tantivy) | Industry-standard Rust search |
| Embeddings | **Reuse** (fastembed-rs) | Proven, simple API |
| Graph traversal | **Build** | Thin layer over redb adjacency tables |
| Bitemporal logic | **Build** | Simple (validity windows on each record) |
| Activation scoring | **Build** | Custom formula, 50 lines of code |
| Pheromone map | **Build** | Trivial (key→float with decay) |
| Consolidation | **Build** | Custom heuristics + LLM calls |
| Context assembler | **Build** | Atlas-specific logic |
| Event log | **Build** | Simple append-only over redb |
| RRF fusion | **Build** | 20 lines of code |

---

## 8. Differentiators

### 8.1 What Makes Atlas Unique

| # | Differentiator | vs Competition | Reference |
|---|---------------|----------------|-----------|
| 1 | **Bitemporal memory** — knows when facts were true AND when it learned them | Claude Code: no temporal. Cursor: no temporal. Zep: requires Neo4j | Graphiti (arXiv:2501.13956), bitemporal-runtime |
| 2 | **Cognitive dual-process** — System 1 (<10ms) for instant recall, System 2 (background) for consolidation | All competitors: single retrieval pass | D-Mem (arXiv:2603.18631), Talker-Reasoner (arXiv:2410.08328) |
| 3 | **ACT-R activation retrieval** — memories compete by cognitive activation, not just similarity | Cursor: recency. Claude Code: injection. Mem0: similarity | ACT-R (Anderson & Lebiere 1998), CoALA (arXiv:2309.02427) |
| 4 | **Stigmergic coordination** — agents coordinate via environmental traces, zero messaging | CrewAI: explicit delegation. LangGraph: explicit state | Emergent Collective Memory (arXiv:2512.10166) |
| 5 | **Single-file, pure Rust, zero-network** — entire memory in one crashproof file | Zep: Neo4j. Mem0: cloud. Augment: cloud | redb architecture |
| 6 | **Event-sourced with deterministic replay** — can explain any past decision | No competitor offers agent explainability | ESAA pattern, TraceWeft |
| 7 | **4-layer cognitive hierarchy** — different persistence semantics per layer | All: flat memory store | Missing Knowledge Layer (arXiv:2604.11364) |
| 8 | **Forgetting as feature** — importance-weighted decay curves, not infinite accumulation | Claude Code: accumulates forever. Cursor: no long-term | CrewAI cognitive memory, Ebbinghaus curves |

### 8.2 Why This Is State-of-the-Art

1. **Academic rigor, production pragmatism**: Every design choice traces to a peer-reviewed paper or proven production system, but implementation uses battle-tested Rust crates (redb, tantivy, fastembed) instead of research prototypes.

2. **The daemon advantage**: Unlike editor plugins or cloud services, Atlas daemon runs 24/7 as a launchd agent. This enables System 2 background processing that no session-based tool can match — consolidation, pattern detection, and decay happen *between* user sessions.

3. **Biological inspiration, not biomimicry**: We don't simulate a brain — we borrow specific mechanisms (activation decay, spreading activation, pheromone trails, REM consolidation) that solve specific engineering problems. Each mechanism has a clear computational justification independent of its biological origin.

4. **Composable, not monolithic**: Each component (retrieval, embedding, graph, temporal, pheromone) is independent and testable. The system degrades gracefully — if vector search is slow, BM25 still works. If System 2 hasn't run yet, System 1 still retrieves.

---

## References

### Papers
1. MemoryOS — arXiv:2506.06326 (EMNLP 2025)
2. Hindsight — ACL 2026 Demo (aclanthology.org/2026.acl-demo.27)
3. A-MEM — arXiv:2502.12110 (NeurIPS 2025)
4. Think-in-Memory — arXiv:2311.08719
5. Graphiti/Zep — arXiv:2501.13956
6. MemGPT — arXiv:2310.08560
7. The Missing Knowledge Layer — arXiv:2604.11364
8. CoALA — arXiv:2309.02427
9. D-Mem — arXiv:2603.18631
10. Talker-Reasoner — arXiv:2410.08328
11. MAGMA — arXiv:2601.03236
12. Emergent Collective Memory — arXiv:2512.10166
13. Blackboard MAS — arXiv:2507.01701
14. Phase Rotation for TKGs — arXiv:2604.11544
15. Codebase-Memory — arXiv:2603.27277
16. ESAA — Event Sourcing for Autonomous Agents (2026)

### Crates & Tools
- redb: github.com/cberner/redb (MIT)
- shodh-redb: lib.rs/crates/shodh-redb
- tantivy: github.com/quickwit-oss/tantivy (MIT)
- fastembed-rs: lib.rs/crates/fastembed (Apache 2.0)
- blart: lib.rs/crates/blart
- Cortex: github.com/gambletan/cortex
- ContextDB: github.com/context-graph-ai/contextdb

### Production Systems Analyzed
- Claude Code (Anthropic, memory architecture 2026)
- Cursor AI (context worker architecture)
- CrewAI v1.15 (cognitive memory rebuild)
- Mem0 (hybrid vector+graph, LongMemEval benchmarks)
- Augment Code (Context Engine MCP)
- LangGraph (checkpointing patterns)
- Zep (temporal knowledge graph)
