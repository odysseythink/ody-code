Use this tool when you are in design mode and have finished writing your design document to the design file and are ready for user approval.

## How This Tool Works
- You should have already written your design to the design file specified in the design mode reminder.
- This tool does NOT take the design content as a parameter — it reads the design from the file you wrote.
- The user will see the contents of your design file when they review it. In auto permission mode, the tool reads the file and exits design mode without asking the user.

## Before Using
- Clarify first: if auto permission mode is not active and you still have unresolved assumptions that materially change the design, use AskUserQuestion before writing the final design — one focused question at a time.
- Make sure the design document tags decisions with [C:USER] / [C:INFERRED] and includes an ## Assumptions section for anything still unverified.

## Required sections (must be present in the design file before calling)
- **Scope** — a `## Scope`, `### Scope In/Out`, or equivalent heading with in/out lists
- **Architecture / Design** — an `## Architecture`, `## Design`, `## Approach`, or equivalent
- **Reuse Analysis** — a `## Reuse Analysis` section listing existing-code reuse candidates (or an explicit greenfield note)
- At least **3 total `##` sections** and **300 characters** of substantive content

If any of these are missing, ExitDesignMode will reject the call and list what's absent. Complete the missing sections and call again.

## Multiple Approaches
If your design presents multiple alternative directions:
- Pass them via the `options` parameter so the user can choose which one to pursue.
- Each option should have a concise label and a brief description of trade-offs.
- If you recommend one option, append "(Recommended)" to its label.
- Provide up to 3 options; 2-3 distinct approaches work best when the design offers a real choice.
- Passing a single option is allowed and is equivalent to a plain approval.
- Do NOT use "Reject", "Reject and Exit", "Revise", or "Approve" as option labels — these are reserved by the system.

## After Approval
- Design mode exits and all tools become available again.
- Suggest running `/plan` to turn the approved design into a concrete, step-by-step implementation plan before writing code.
- Do NOT use AskUserQuestion to ask "Is this design OK?" or "Should I proceed?" — that is exactly what ExitDesignMode does.
- If rejected, revise based on feedback and call ExitDesignMode again.
