# MCL vs Alternatives — Honest Comparison

This document compares MCL to five frameworks that overlap with its problem space. MCL is not the right tool for every job. The goal here is to help you figure out if it fits yours.

## The Short Version

MCL is a declarative language and Rust runtime for building applications where an LLM narrates and a deterministic engine enforces rules. Its sweet spot is interactive fiction, educational simulations, and any domain where the LLM must stay within declared knowledge boundaries.

If you need a general-purpose agent framework, use LangGraph. If you need to constrain LLM output at the token level, use Guidance. If you need prompt optimization, use DSPy. MCL is for when you want to define a world in text files and have an LLM bring it to life without breaking the rules.

## Comparison Matrix

| Dimension | MCL | LangGraph | LMQL | Guidance | DSPy | Rivet |
|-----------|-----|-----------|------|----------|------|-------|
| Definition language | Bespoke (.mcl) | Python | SQL-like DSL | Jinja-like | Python | Visual (node editor) |
| Deterministic replay | Yes (Pregel super-steps) | Partial | Partial | No | No | No |
| Knowledge boundaries | Per-node with glob patterns | No | No | No | No | No |
| Signal protocol | Inline `MCL::*` extraction | N/A (callback-based) | N/A (typed variables) | N/A (template-based) | N/A (module-based) | N/A (wire-based) |
| Setup complexity | Medium (Rust, TOML/MCL files) | Low (pip install) | Low (pip install) | Low (pip install) | Low (pip install) | Medium (Electron app) |
| Multi-provider | Ollama, OpenAI, Anthropic | Any (Python callbacks) | Any (LiteLLM) | Any (Python callbacks) | Any (LiteLLM) | Any (REST calls) |
| Streaming | Yes (backend support exists) | Yes | Yes | Yes | No | Yes |
| Language | Rust | Python | Python | Python | Python | TypeScript |
| Execution model | Pregel graph | State machine graph | Single-pass query | Single-pass template | Optimizer pipeline | Visual dataflow |

## Detailed Comparisons

### MCL vs LangGraph

LangGraph is the most direct competitor. Both use a graph execution model. Both connect LLM calls to deterministic logic.

**Where LangGraph wins:**

LangGraph is a Python library with a huge ecosystem. You can pip install it and have a working agent in 20 lines. It integrates with LangChain's tool ecosystem (hundreds of pre-built tools). Its state machine model is familiar and well-documented. Python's dynamic nature means you can prototype fast and change things without recompiling.

```python
# LangGraph: 15 lines for a basic agent loop
from langgraph.graph import StateGraph, END

def agent(state):
    response = llm.invoke(state["messages"])
    return {"messages": [response]}

def should_continue(state):
    if state["messages"][-1].tool_calls:
        return "tools"
    return END

graph = StateGraph(AgentState)
graph.add_node("agent", agent)
graph.add_node("tools", tool_node)
graph.add_conditional_edges("agent", should_continue)
graph.add_edge("tools", "agent")
app = graph.compile()
```

**Where MCL wins:**

MCL gives you deterministic replay. The Pregel super-step model means the same inputs always produce the same state mutations (excluding the LLM's nondeterministic output). LangGraph's async message passing has no ordering guarantees.

Knowledge boundaries are a first-class concept in MCL. You declare what the narrator knows and explicitly does not know per context. LangGraph has no equivalent; you'd build it ad-hoc with prompt engineering.

MCL's declarative format separates world definition from code. A game designer who doesn't write Rust can create rooms, entities, and actions in `.mcl` files. LangGraph forces everything into Python.

**Equivalent functionality: Lines of code**

Building a 3-room interactive fiction game with combat, inventory, and room transitions:
- MCL: ~250 lines across 3 `.mcl` files + 1 `.graph.toml` (74 lines)
- LangGraph: ~400-600 lines of Python (state definitions, node functions, edge wiring, prompt templates, tool definitions)

**Verdict:** Use LangGraph for general agent workflows. Use MCL when you need deterministic execution, knowledge boundaries, and a declarative format for non-programmers.

### MCL vs LMQL

LMQL (Language Model Query Language) constrains LLM output using a SQL-like syntax with type constraints on variables.

**Where LMQL wins:**

LMQL operates at the token level. Its constraints are enforced during generation, not after. If you declare `answer: str[5 words]`, the model literally cannot produce more than 5 words. This is fundamentally more reliable than MCL's post-hoc signal extraction.

```python
# LMQL: guaranteed structure at generation time
@lmql.query
def tell_joke(topic):
    '''lmql
    "Tell me a joke about {topic}.\\n"
    "Joke: [JOKE]" where len(JOKE) < 200
    "Rating: [RATING]" where RATING in ["G", "PG", "R"]
    return JOKE, RATING
    '''
```

LMQL supports branching, constraints on partial output, and early stopping. Its type system catches errors before the LLM finishes generating.

**Where MCL wins:**

MCL is not trying to constrain token generation. It constrains behavior over multiple turns in a persistent world. LMQL has no concept of state, entities, transitions, or persistent sessions. Each LMQL query is stateless.

MCL's knowledge boundary system has no equivalent in LMQL. LMQL can constrain what the model says in one response, but it cannot declare "the narrator does not know about the treasure room" across an entire session.

The signal protocol allows freeform narration with embedded structured data. LMQL forces the entire output into a structured template, which produces worse narrative quality.

**Verdict:** Use LMQL when you need strict output structure from a single LLM call. Use MCL when you need constrained behavior across a multi-turn interactive session.

### MCL vs Guidance

Guidance (from Microsoft) uses a Jinja-like template language to control LLM generation with interleaved text and programmatic blocks.

**Where Guidance wins:**

Like LMQL, Guidance operates at the generation level. It can enforce grammars, JSON schemas, and even character-level constraints during token generation. This is more reliable than MCL's post-hoc extraction for structured output.

```python
# Guidance: guaranteed valid JSON output
@guidance
def game_action(llm):
    action = llm + '''Choose an action:
    {{#select 'action'}}Attack{{or}}Defend{{or}}Flee{{/select}}'''
    return action
```

Guidance's `select` blocks guarantee the model picks from a fixed set of options. MCL cannot make this guarantee; the LLM might emit `MCL::action(invented_thing)` which the parser would flag as invalid.

Guidance works as a Python library with minimal setup. It supports any HuggingFace model out of the box.

**Where MCL wins:**

MCL provides a complete application framework: state management, entity tracking, transitions, combat resolution, inventory systems. Guidance is a prompting library. You'd need to build all of that yourself on top of Guidance.

The declarative `.mcl` file format means non-programmers can define game worlds. Guidance requires Python code for everything.

MCL's node-graph execution model supports multiple concurrent nodes communicating through typed channels. Guidance is single-chain execution.

**Verdict:** Use Guidance when you need fine-grained control over LLM token generation. Use MCL when you need a complete framework for LLM-powered interactive applications.

### MCL vs DSPy

DSPy takes a fundamentally different approach: instead of hand-writing prompts, you define signatures and it optimizes them automatically.

**Where DSPy wins:**

DSPy can automatically improve your prompts. You define what you want (input/output signatures), provide training examples, and DSPy finds the best prompt wording. For production systems, this optimization is valuable. No other framework here does this.

```python
# DSPy: automatic prompt optimization
class GameNarrator(dspy.Signature):
    """Narrate a game scene given the current state."""
    scene = dspy.InputField()
    action = dspy.InputField()
    narration = dspy.OutputField()

narrator = dspy.ChainOfThought(GameNarrator)
optimizer = dspy.MIPROv2(metric=quality_score)
optimized = optimizer.compile(narrator, trainset=examples)
```

DSPy also provides built-in evaluation metrics and a systematic approach to improving LLM behavior over time.

**Where MCL wins:**

DSPy optimizes individual prompts. MCL constrains an entire interactive system. There is no concept in DSPy of rooms, entities, transitions, combat systems, or knowledge boundaries. DSPy could optimize the narration prompt for an MCL system, but it cannot replace MCL.

MCL's signal protocol provides a structured interface between the LLM and the game engine that DSPy has no equivalent for. DSPy modules return typed outputs, but those outputs are not embedded in freeform narration.

MCL runs locally with Ollama. DSPy optimization typically requires many API calls, making it expensive with local models.

**Verdict:** Use DSPy to optimize prompts. Use MCL to build the application around the LLM. They solve different problems.

### MCL vs Rivet

Rivet is a visual node editor for building LLM pipelines. You drag nodes onto a canvas, connect them with wires, and configure properties in a GUI.

**Where Rivet wins:**

Rivet's visual interface is excellent for prototyping and debugging. You can see the entire dataflow at a glance, inspect intermediate values, and iterate quickly. For teams with non-engineers, the visual metaphor is more accessible than text files.

Rivet supports a wide range of node types out of the box: text, JSON, image, function calling, loops, and more. Its plugin system lets you extend it with custom nodes in TypeScript.

The debugging experience in Rivet is superior. You can step through execution, see exactly what data flows between nodes, and inspect the full prompt chain.

**Where MCL wins:**

MCL's `.mcl` files are text, which means they work with version control, code review, diff tools, and CI/CD. Rivet projects are JSON files that are difficult to diff and review.

MCL provides domain-specific abstractions: entities, actions, transitions, knowledge boundaries. Rivet provides generic dataflow primitives. Building a game in Rivet means implementing game logic from scratch in custom TypeScript nodes.

MCL's Pregel executor gives deterministic ordering within super-steps. Rivet's execution model depends on the graph topology and has no ordering guarantees for parallel branches.

MCL runs in the terminal. Rivet requires an Electron application.

**Verdict:** Use Rivet for visual prototyping and debugging of LLM pipelines. Use MCL for production interactive applications where text-based definition and deterministic execution matter.

## When to Use MCL

MCL is the right choice when:

1. You need the LLM to stay within declared knowledge boundaries across multiple turns
2. You want deterministic replay of game state (same inputs, same state mutations)
3. Non-programmers need to define content (rooms, entities, actions) in text files
4. You need a complete application framework, not just a prompting library
5. You're building interactive fiction, educational simulations, or similar constrained-world applications

MCL is the wrong choice when:

1. You need token-level output constraints (use Guidance or LMQL)
2. You need a general-purpose agent framework (use LangGraph)
3. You want automatic prompt optimization (use DSPy)
4. You prefer visual programming over text files (use Rivet)
5. You need Python ecosystem integration (MCL is Rust)
6. You need the LLM to produce perfectly structured output every time (MCL's signal protocol relies on prompt engineering, not token-level enforcement)

## Honest Assessment of Signal Reliability Risk

MCL's biggest architectural risk is the inline signal protocol. The entire system depends on LLMs reliably emitting `MCL::action(attack, target=skeleton_guard)` within freeform narration. This is not enforced at the token level. The LLM could:

- Narrate an action without emitting a signal
- Emit a malformed signal
- Emit a signal for an action not in the available list
- Invent new signal types

MCL mitigates this with few-shot examples in the system prompt, retry-on-missing-signal logic, and a structured JSON fallback mode. But the base reliability varies by model and has not been measured across the major providers.

Every competitor except LangGraph solves output structure at the generation level. MCL chooses to solve it at the application level, trading raw reliability for better narrative quality. If signal reliability proves too low in practice, MCL falls back to structured output mode, which sacrifices narration quality for correctness.

This tradeoff is intentional but honest: it may not work well enough with every model. Test with your target model before committing to MCL.
