Use this tool when a request is vague, open-ended, or exploratory and needs a design / brainstorming pass before any implementation planning.

Design mode is the brainstorming sibling of plan mode. Use it when ANY of these apply:

1. Ambiguous or under-specified goals — the user described an outcome, not a solution.
2. Genuinely open design space — several meaningfully different approaches exist and the right one depends on user preferences, constraints, or context you don't yet have.
3. Decisions that are expensive to reverse — architecture, data model, public API, product behavior.
4. Greenfield or net-new features where requirements must be clarified before scoping.

Prefer EnterPlanMode (not this tool) when the WHAT is already clear and you only need to lay out implementation steps. Design mode is for deciding WHAT and WHY; plan mode is for HOW.

Permission mode notes:
- EnterDesignMode enters design mode automatically without an approval prompt in all permission modes.
- In yolo and manual modes, ExitDesignMode presents the design to the user for approval.
- In auto permission mode, do not use AskUserQuestion; make the best decision from available context and record open questions in the ## Assumptions section instead.

## What Happens in Design Mode
1. Clarify first — surface your assumptions and resolve the ones that materially change the design by asking the user with AskUserQuestion, one focused question at a time. Do NOT converge on a solution before the key unknowns are settled.
2. Investigate the codebase with read-only tools (Read, Grep, Glob). As part of that investigation, run an internal reuse scan: look for existing functions, types, or modules that already solve the problem. Use `Agent(subagent_type="explore")` for non-trivial investigation. Use Bash only when needed.
3. Explore 2-3 genuinely different approaches and weigh their trade-offs — do not pad with trivial variations.
4. Write the design document to the design file with Write or Edit. Tag each decision [C:USER] or [C:INFERRED], and include an ## Assumptions section for anything still unverified.
5. Present the design via ExitDesignMode for approval. After approval, suggest `/plan` to turn the chosen direction into a concrete implementation plan.

Optional parameter:
- `topic` — A short topic phrase (2–5 words) to include in the design filename. If omitted, the topic is inferred automatically from the conversation.
