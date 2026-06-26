# Phase B — Skills: `idea-generator` and `idea-evaluator`

This phase ports the upstream `.skill` packages into the built-in skill registry. It is independent of Phase A and can run in parallel with it. Each skill task is test-first: the skill wrapper is added to the existing builtin-skills test as it is created.

---

### Task B4: Add `idea-generator` built-in skill

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/skill/builtin/idea-generator.md`
- Create: `packages/agent-core/src/skill/builtin/idea-generator.ts`
- Modify: `packages/agent-core/test/skill/builtin-skills.test.ts`

This task ports the upstream `idea-generator.skill/SKILL.md` content into a built-in skill that is hidden in all non-normal session modes.

- [ ] **Write the failing test.** Modify `packages/agent-core/test/skill/builtin-skills.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { SkillRegistry } from '../../src/skill/registry';
import {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  IDEA_GENERATOR_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
} from '../../src/skill/builtin';

const BUILTIN_SKILLS = [
  { skill: DISPATCHING_PARALLEL_AGENTS_SKILL, name: 'dispatching-parallel-agents' },
  { skill: EXECUTING_PLANS_SKILL, name: 'executing-plans' },
  { skill: FINISHING_A_DEVELOPMENT_BRANCH_SKILL, name: 'finishing-a-development-branch' },
  { skill: IDEA_GENERATOR_SKILL, name: 'idea-generator' },
  { skill: MCP_CONFIG_SKILL, name: 'mcp-config' },
  { skill: RECEIVING_CODE_REVIEW_SKILL, name: 'receiving-code-review' },
  { skill: REQUESTING_CODE_REVIEW_SKILL, name: 'requesting-code-review' },
  { skill: SIMPLICITY_FIRST_SKILL, name: 'simplicity-first' },
  { skill: SUBAGENT_DRIVEN_DEVELOPMENT_SKILL, name: 'subagent-driven-development' },
  { skill: SYNC_CHANGELOG_SKILL, name: 'sync-changelog' },
  { skill: SYSTEMATIC_DEBUGGING_SKILL, name: 'systematic-debugging' },
  { skill: TEST_DRIVEN_DEVELOPMENT_SKILL, name: 'test-driven-development' },
  { skill: USING_GIT_WORKTREES_SKILL, name: 'using-git-worktrees' },
  { skill: VERIFICATION_BEFORE_COMPLETION_SKILL, name: 'verification-before-completion' },
];

describe('built-in skills', () => {
  it('has exactly 14 built-in skills', () => {
    expect(BUILTIN_SKILLS).toHaveLength(14);
  });

  it.each(BUILTIN_SKILLS)('skill "$name" has correct metadata', ({ skill, name }) => {
    expect(skill.name).toBe(name);
    expect(skill.source).toBe('builtin');
    expect(skill.path).toBe(`builtin://${name}`);
    expect(skill.dir).toBe(`builtin://${name}`);
    expect(skill.content.length).toBeGreaterThan(0);
    expect(skill.description.length).toBeGreaterThan(0);
  });

  it('all skills are sorted alphabetically in listSkills', () => {
    const registry = new SkillRegistry();
    for (const { skill } of BUILTIN_SKILLS) {
      registry.registerBuiltinSkill(skill);
    }
    const listed = registry.listSkills();
    const names = listed.map((s) => s.name);
    const sorted = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sorted);
  });
});
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected failure: `IDEA_GENERATOR_SKILL` cannot be imported from `../../src/skill/builtin`.

- [ ] **Write the minimal implementation.**

Create `packages/agent-core/src/skill/builtin/idea-generator.md` with this frontmatter followed by the upstream body:

```markdown
---
type: inline
name: idea-generator
description: >
  Systematically generate startup idea candidates using 7 proven recipes adapted for the user's
  specific context (skills, industry exposure, connections, assets). Use this skill whenever the user
  is stuck in "don't know what to build" mode, says "I need ideas", "what should I work on",
  "help me brainstorm directions", "I'm not finding anything", or has been in open exploration for
  2+ weeks without locking a direction. Also trigger when the user finishes a round of idea-evaluator
  scoring and all candidates scored ≤4, indicating the current idea pool is exhausted and needs
  replenishment. This skill outputs a ranked list of concrete idea candidates ready to be fed into
  idea-evaluator. It is a DIVERGENT tool (expand options) that pairs with idea-evaluator (CONVERGENT
  tool, narrow options). Do not use this skill if the user already has a clear direction — use
  idea-evaluator instead.
hiddenInModes:
  - plan
  - design
  - office-hours
  - game-design
---

# SKILL: Idea Generator (想法发生器) v1

## Position in Decision Flow

```
没方向 → [Idea Generator 想法发生器] → 候选清单
                                        ↓
                                   [Idea Evaluator 想法质检] → 评分≥6 → 六模式审评
```

This skill answers: **"What directions should I even be looking at?"**
Idea Evaluator answers: "Is this specific idea any good?"

---

## Pre-Generation: Context Inventory (上下文盘点)

Before running the 7 recipes, inventory the user's available inputs. Each input is fuel for idea generation — the more specific, the better the output.

```yaml
inventory_checklist:
  skills:
    - Core technical skills (languages, frameworks, domains)
    - Non-technical skills (sales, writing, domain knowledge, languages spoken)
    - Rare skill combinations (e.g., "Go + quantitative finance + AI agents")

  industry_exposure:
    - Current job industry and daily workflow observations
    - Previous job industries
    - Outsourcing/freelance client industries
    - Industries of close friends/family

  pain_points_observed:
    - Problems you personally experience repeatedly
    - Problems you've seen colleagues/clients struggle with
    - Complaints you've heard from specific people (not abstract "market needs")
    - Broken workflows you've witnessed firsthand

  assets:
    - Existing code/products (even frozen ones)
    - Existing user base (even small)
    - Domain knowledge accumulated
    - Relationships/access to specific user groups

  recent_changes:
    - New technologies that just became viable
    - Regulatory changes affecting industries you know
    - Behavioral shifts you've noticed (post-COVID, AI adoption, etc.)
    - Industry disruptions in your orbit
```

**Output**: A filled context card. This card is referenced by every recipe below.

---

## The 7 Recipes

### Recipe 1: Start From Your Own Problems (从自身痛点出发)

```yaml
method: >
  List every friction/annoyance/inefficiency you personally encounter
  in a typical week — at work, at home, managing finances, learning,
  communicating, building software, managing projects.

prompt_questions:
  - "What did you do this week that felt unnecessarily painful or slow?"
  - "What task do you keep postponing because it's annoying?"
  - "What manual process do you repeat that should be automated?"
  - "What tool do you use daily that frustrates you?"

quality_filter: >
  Best results come from problems you experience as a PROFESSIONAL,
  not as a consumer. Consumer problems attract massive competition.
  Professional/B2B problems are often invisible to outsiders.

output: List of [problem + who else has it + current workaround]
```

### Recipe 2: Start From What You're Uniquely Good At (从独特能力出发)

```yaml
method: >
  Identify your rare skill combinations. A single skill (Go, AI, frontend)
  is common. Two skills combined (Go + AI) is less common. Three skills
  combined (Go + AI + quantitative trading infrastructure) is rare.
  Look for problems that sit at the intersection of your rare combination.

prompt_questions:
  - "What can you build in a weekend that would take most teams a month?"
  - "What do people ask you for help with that they can't easily get elsewhere?"
  - "What technical problem have you solved that most engineers haven't faced?"

⚠️ SISP guard: >
  This recipe is the highest-risk for SISP (Bug 5). The output must be
  [capability → problem it solves → who has that problem], NOT
  [capability → cool thing I could build]. If you can't name a specific
  person with the problem, the idea is SISP. Flag it and move on.

output: List of [rare capability intersection + specific problem it solves + for whom]
```

### Recipe 3: Start From Changes You've Noticed (从你观察到的变化出发)

```yaml
method: >
  What has recently changed in technology, regulation, behavior, or
  industry structure that creates a new gap? The best startup timing
  comes from riding a wave that just started — not one that's peaking.

prompt_questions:
  - "What became possible in the last 12 months that wasn't before?"
  - "What industry is being forced to change by new regulation or technology?"
  - "What behavior shift have you observed in people around you?"
  - "What used to be expensive/hard that is now cheap/easy?"

timing_check: >
  Cross-ref with Model 06 (Contrarian Timing). If "everyone" already
  knows about this change, you're late. Look for changes that are
  obvious to insiders but invisible to outsiders.

output: List of [change + new gap created + who is affected + timing assessment]
```

### Recipe 4: Start From Industries You're Inside (从你接触的行业出发)

```yaml
method: >
  Your current job, freelance clients, and personal network give you
  insider access to specific industries. Most outsiders can't see the
  real problems inside these industries. This is your information
  advantage — use it.

prompt_questions:
  - "What do people in your industry complain about at lunch?"
  - "What process at your company is shockingly manual or outdated?"
  - "What data exists in your industry that no one is using well?"
  - "What do your outsourcing clients keep asking for that isn't a product yet?"
  - "What would make your current job 10x easier?"

⚠️ schlep_check: >
  Industry problems often involve schlep — messy integrations, legacy
  systems, regulatory compliance, relationship-dependent sales.
  If you feel yourself dismissing an idea because "it's too industry-specific"
  or "not techy enough", that's Bug 6 (Schlep Blindness). Flag it
  and force yourself to score it anyway.

output: List of [industry + specific problem + who owns budget for this + how they cope now]
```

### Recipe 5: Start From Recent Tech Breakthroughs (从技术突破出发)

```yaml
method: >
  A new technology capability (LLMs, AI agents, multimodal models,
  cheap inference, voice AI, etc.) has just made something possible
  that wasn't before. What specific workflow can now be 10x better?

prompt_questions:
  - "What task required a human expert last year that AI can now handle?"
  - "What was too expensive to automate before but is now cheap?"
  - "What data was unstructured/unusable before but LLMs can now parse?"
  - "What manual QA/review/analysis process can now be automated?"

⚠️ SISP guard (strict): >
  This recipe has the HIGHEST SISP risk. The question is NOT
  "what can AI do?" but "what painful workflow can AI fix for
  a specific person who will pay?" If your answer starts with
  "AI can..." instead of "[Person X] struggles with...", stop.
  Reverse the direction.

⚠️ tar_pit guard: >
  "AI + [broad category]" is almost always a tar pit.
  "AI writing assistant", "AI study tool", "AI personal assistant"
  — these are graveyards. Be specific: AI for [specific role]
  doing [specific task] in [specific industry].

output: List of [new capability + specific workflow improved + who benefits + why now]
```

### Recipe 6: Talk to People (跟人聊)

```yaml
method: >
  Structured conversations with potential users — not friends, not
  other developers. People who do real work in real industries.
  The goal is NOT to pitch an idea. The goal is to discover problems.

conversation_template:
  1. "What's the most tedious part of your job?"
  2. "What tools do you use daily? What do you hate about them?"
  3. "If you could wave a magic wand and fix one thing, what would it be?"
  4. "How much time/money does [that problem] cost you per month?"
  5. "Have you tried to fix it? What happened?"

who_to_talk_to:
  - Your outsourcing clients (you already have access)
  - Colleagues at your current company
  - People in WeChat/Telegram industry groups
  - Friends who work in non-tech industries (they have unsolved problems tech people never see)

minimum_bar: >
  Talk to 5 people outside your immediate circle before concluding
  "there are no good ideas". If you haven't talked to 5, you haven't
  looked — you've just thought.

output: List of [person's role + their problem + severity + current solution + willingness to pay]
```

### Recipe 7: Find Incumbent Weaknesses (找巨头的弱点)

```yaml
method: >
  Large companies and established products always have blind spots.
  They can't serve small niches profitably. They can't move fast.
  They can't customize. They can't care about individual users.
  Find a specific segment they're underserving.

prompt_questions:
  - "What large software product do people in [industry] use reluctantly?"
  - "What's the most common complaint about [dominant tool]?"
  - "Who is being overcharged by [incumbent] relative to the value they get?"
  - "What user segment is too small for [big company] to care about?"

quality_signal: >
  The best answers come from actual user complaints — app store reviews,
  Reddit threads, V2EX posts, industry forums. Not from your imagination.

output: List of [incumbent + their weak point + underserved segment + what "good enough" alternative looks like]
```

---

## Post-Generation: Candidate Assembly

After running relevant recipes (not all 7 are needed every time — pick 3-4 most relevant based on the context inventory), assemble candidates.

### Deduplication & Clustering

Group similar ideas together. Often multiple recipes point at the same underlying opportunity from different angles — that's a strong signal.

### Quick Viability Filter

Before sending to idea-evaluator, apply a 3-second gut check on each candidate:

```yaml
kill_if:
  - You can't describe who pays and why in one sentence
  - The only user you can imagine is "everyone"
  - You feel excited about the TECHNOLOGY but bored by the USER PROBLEM
  - You've seen this exact idea fail 3+ times (tar pit)

keep_if:
  - You can name a specific person who has this problem
  - The idea makes you slightly uncomfortable (schlep signal)
  - You can see how to test it in <48 hours
  - Multiple recipes converged on this direction
```

---

## Output Format

```
## 想法生成报告 (Idea Generation Report)

### 上下文盘点
- 技能栈: [summary]
- 行业接触: [summary]
- 已观察痛点: [summary]
- 已有资产: [summary]
- 近期变化: [summary]

### 使用的Recipes
[List which recipes were run and why]

### 候选清单

| # | 想法 | 来源Recipe | 目标用户 | 核心痛点 | Schlep指数 | 初筛 |
|---|------|-----------|---------|---------|-----------|------|
| 1 | [idea] | R[X] | [who] | [pain] | 高/中/低 | ✅/❌ |
| 2 | ... | ... | ... | ... | ... | ... |

### 收敛信号
[哪些方向被多个recipe指向？这些是最值得优先评估的]

### 下一步
对初筛通过的候选，逐个运行 idea-evaluator 想法质检。
建议优先评估: #[X] 和 #[Y]（理由）
```

---

## Usage Notes

- **不需要每次跑全部7个recipe**。根据上下文盘点结果，选3-4个最有燃料的recipe跑。
- **Recipe 4（行业内部）和 Recipe 6（跟人聊）产出质量通常最高**，因为它们基于真实信息而非推测。
- **Recipe 2 和 Recipe 5 的SISP风险最高**，每个产出都要过SISP检测。
- **Schlep指数高的想法不是坏想法**——恰恰相反，它们往往竞争更少。标注它是为了让你正视它，而不是过滤它。
- **如果跑完所有recipe产出为零**，问题不是"没有好想法"，而是你的观察输入不够。回到Recipe 6，跟5个真实用户聊。
- **每轮生成建议间隔2周**。连续生成想法不会提高质量——中间需要新的信息输入（工作观察、用户对话、行业动态）才能产出不同的候选。
```

Create `packages/agent-core/src/skill/builtin/idea-generator.ts`:

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import IDEA_GENERATOR_BODY from './idea-generator.md';

const PSEUDO_PATH = 'builtin://idea-generator';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/idea-generator.md',
  skillDirName: 'idea-generator',
  source: 'builtin',
  text: IDEA_GENERATOR_BODY,
});

export const IDEA_GENERATOR_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
```

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected: tests pass with 14 built-in skills.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/skill/builtin/idea-generator.md packages/agent-core/src/skill/builtin/idea-generator.ts packages/agent-core/test/skill/builtin-skills.test.ts
git commit -m "feat(agent-core): add idea-generator built-in skill"
```

---

### Task B5: Add `idea-evaluator` built-in skill

**Depends on:** Task B4

**Files:**
- Create: `packages/agent-core/src/skill/builtin/idea-evaluator.md`
- Create: `packages/agent-core/src/skill/builtin/idea-evaluator.ts`
- Modify: `packages/agent-core/test/skill/builtin-skills.test.ts`

This task ports the upstream `idea-evaluator.skill/SKILL.md` content and updates the skill count to 15.

- [ ] **Write the failing test.** Update `packages/agent-core/test/skill/builtin-skills.test.ts`:

```typescript
import { describe, expect, it } from 'vitest';
import { SkillRegistry } from '../../src/skill/registry';
import {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  IDEA_EVALUATOR_SKILL,
  IDEA_GENERATOR_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
} from '../../src/skill/builtin';

const BUILTIN_SKILLS = [
  { skill: DISPATCHING_PARALLEL_AGENTS_SKILL, name: 'dispatching-parallel-agents' },
  { skill: EXECUTING_PLANS_SKILL, name: 'executing-plans' },
  { skill: FINISHING_A_DEVELOPMENT_BRANCH_SKILL, name: 'finishing-a-development-branch' },
  { skill: IDEA_EVALUATOR_SKILL, name: 'idea-evaluator' },
  { skill: IDEA_GENERATOR_SKILL, name: 'idea-generator' },
  { skill: MCP_CONFIG_SKILL, name: 'mcp-config' },
  { skill: RECEIVING_CODE_REVIEW_SKILL, name: 'receiving-code-review' },
  { skill: REQUESTING_CODE_REVIEW_SKILL, name: 'requesting-code-review' },
  { skill: SIMPLICITY_FIRST_SKILL, name: 'simplicity-first' },
  { skill: SUBAGENT_DRIVEN_DEVELOPMENT_SKILL, name: 'subagent-driven-development' },
  { skill: SYNC_CHANGELOG_SKILL, name: 'sync-changelog' },
  { skill: SYSTEMATIC_DEBUGGING_SKILL, name: 'systematic-debugging' },
  { skill: TEST_DRIVEN_DEVELOPMENT_SKILL, name: 'test-driven-development' },
  { skill: USING_GIT_WORKTREES_SKILL, name: 'using-git-worktrees' },
  { skill: VERIFICATION_BEFORE_COMPLETION_SKILL, name: 'verification-before-completion' },
];

describe('built-in skills', () => {
  it('has exactly 15 built-in skills', () => {
    expect(BUILTIN_SKILLS).toHaveLength(15);
  });

  it.each(BUILTIN_SKILLS)('skill "$name" has correct metadata', ({ skill, name }) => {
    expect(skill.name).toBe(name);
    expect(skill.source).toBe('builtin');
    expect(skill.path).toBe(`builtin://${name}`);
    expect(skill.dir).toBe(`builtin://${name}`);
    expect(skill.content.length).toBeGreaterThan(0);
    expect(skill.description.length).toBeGreaterThan(0);
  });

  it('all skills are sorted alphabetically in listSkills', () => {
    const registry = new SkillRegistry();
    for (const { skill } of BUILTIN_SKILLS) {
      registry.registerBuiltinSkill(skill);
    }
    const listed = registry.listSkills();
    const names = listed.map((s) => s.name);
    const sorted = [...names].sort((a, b) => a.localeCompare(b));
    expect(names).toEqual(sorted);
  });
});
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected failure: `IDEA_EVALUATOR_SKILL` cannot be imported.

- [ ] **Write the minimal implementation.**

Create `packages/agent-core/src/skill/builtin/idea-evaluator.md` with this frontmatter followed by the upstream body:

```markdown
---
type: inline
name: idea-evaluator
description: >
  Evaluate the quality of a startup or product idea BEFORE committing to the six-model review.
  Use this skill whenever the user describes a new startup idea, product concept, or business direction
  they're considering — especially during the P1 open exploration phase. Also trigger when the user says
  "I have an idea", "what do you think of this direction", "should I explore X", "I noticed a problem
  with Y", or presents any concept that could become a product or business. This skill runs 10 evaluation
  questions, 4 mistake checks, 3 counter-intuitive signal checks, and outputs a structured quality score.
  The output feeds directly into the six-model review: ideas scoring ≥6/10 proceed to full review,
  ideas scoring 3-5 get specific improvement suggestions, ideas scoring ≤2 get a recommendation to
  drop or radically rethink. Always use this skill BEFORE the user starts building, designing architecture,
  or running the six-model review on a new idea. This is the first gate in the funnel.
hiddenInModes:
  - plan
  - design
  - office-hours
  - game-design
---

# SKILL: Idea Evaluator (想法质检) v1

## Position in Decision Flow

```
新想法 → [Idea Evaluator 想法质检] → 评分≥6 → 六模式审评 → 执行
                                   → 评分3-5 → 改进建议 → 重新评估
                                   → 评分≤2 → 建议放弃或彻底重构
```

This skill answers: **"Is this idea worth putting through the full review?"**
The six-model review answers: "Should I do it and how?"
MVP First answers: "How do I validate it cheaply?"

---

## Phase 1: Mistake Scan (4项排雷)

Before scoring, check whether the idea has any of these structural problems. Any hit = mandatory warning before proceeding.

### M1: SISP — 拿方案找问题 (Solution In Search of a Problem)

```yaml
detection:
  - User starts from technology ("I can use X to do Y")
  - User starts from capability ("I know how to build X, what can I do with it?")
  - The "problem" was found AFTER the solution was conceived
  - Cross-ref: Bug 5 SISP variant

warning: >
  ⚠️ SISP detected. You started from a technology/capability, not from a pain point.
  Reverse the question: Who is in pain? How bad? How are they coping now?
  If you can't answer those three without referencing your tech stack, this is SISP.
```

### M2: Tar Pit — 结构性死胡同

```yaml
detection:
  - Problem is real, solution sounds reasonable, but many have tried and failed
  - Classic tar pits: social plans apps, restaurant discovery, general AI assistants,
    "Uber for X" without network density, todo apps, podcast discovery
  - Signal: you can easily find 5+ dead startups that tried the same thing

action: >
  Search for predecessors. If 3+ similar attempts failed for similar structural reasons
  (not just bad execution), this is a tar pit. State the structural reason explicitly.
  User must explain why their approach avoids this specific structural trap.
```

### M3: Evaluation Laziness — 没做功课

```yaml
detection:
  - User cannot name a single competitor or existing alternative
  - User has not talked to any potential user
  - User says "I think people need this" with zero evidence

warning: >
  ⚠️ You haven't done basic homework. Before scoring this idea:
  1. Google "[your idea] + startup/app/tool" — what comes up?
  2. Name 3 people who have this problem. Have you talked to any of them?
  3. What do people currently use to solve this? (The answer is never "nothing")
```

### M4: Waiting for Perfect — 等待完美想法

```yaml
detection:
  - User has been exploring for weeks without committing to test any direction
  - User keeps generating new ideas but never validates any
  - User says "this one isn't good enough either"

warning: >
  ⚠️ You're in idea-shopping mode. Good ideas often don't look good at first.
  Airbnb sounded terrible. Stripe sounded boring. Pick the best of what you have
  and run a 48-hour validation test. A tested mediocre idea > an untested brilliant one.
```

---

## Phase 2: Ten-Question Scoring (10问评分)

Score each question 0/1/2. Total range: 0-20, normalized to 0-10 for final score.

### Q1: Founder-Market Fit (创始人-市场匹配)

```
2 = You have deep domain experience or are the target user yourself
1 = You have adjacent experience or strong technical fit
0 = No connection to this market/problem — you chose it because it "seems big"
```

### Q2: Market Size (市场规模)

```
2 = Large existing market OR small market growing rapidly
1 = Medium market with clear expansion path
0 = Niche with no growth path, or "huge market" you can't realistically capture
```

### Q3: Problem Acuteness (问题急迫度)

```
2 = Hair-on-fire problem — users actively spending money/time to solve it NOW
1 = Real annoyance — users complain but tolerate it
0 = Nice-to-have — users don't lose sleep over this
```

### Q4: Competitive Landscape (竞争格局)

```
2 = Competitors exist but you have a specific insight they're missing
1 = No direct competitors (could mean untapped OR no market)
0 = Strong incumbents with no clear differentiation for you
```

### Q5: Personal Desire (自己想用)

```
2 = You would be a daily/weekly user of this product
1 = You can see yourself using it occasionally
0 = You would never use this — you're building for a user you don't understand
```

### Q6: Timing — Recently Possible or Necessary (时机)

```
2 = New technology/regulation/behavior shift just made this possible or urgent
1 = Gradual trend moving in your favor
0 = Could have been built 5 years ago — why wasn't it? (Answer matters)
```

### Q7: Proxy Validation (有无参照物)

```
2 = Similar model proven in another market/geography (your version for your context)
1 = Partial proxy — some elements validated elsewhere
0 = Completely novel — no evidence this model works anywhere
```

### Q8: Long-term Commitment (长期投入意愿)

```
2 = Genuinely excited to work on this for 2+ years
1 = Willing to commit 6-12 months to see if it works
0 = Already bored thinking about it — just want quick money
```

### Q9: Scalability (可规模化)

```
2 = Clear path to serve 100x users with <10x effort (software/platform)
1 = Can scale with moderate effort increase (productized service)
0 = Revenue scales linearly with your time (pure consulting/freelance)
```

### Q10: Idea Space Quality (赛道质量)

```
2 = Rich idea space — even if this specific angle fails, adjacent pivots exist
1 = Some pivot room but limited
0 = Dead-end — if this specific implementation fails, nothing else to try
```

---

## Phase 3: Counter-Intuitive Signal Check (3个反直觉加分项)

These are POSITIVE signals that most people misread as negative. Each adds +1 to the raw score.

### S1: Hard to Get Started (启动困难)

```
+1 if: The idea requires significant schlep to get going — regulatory hurdles,
       complex integrations, cold-start problems, industry relationships needed.
Why positive: High barrier to entry = high barrier for competitors too.
Cross-ref: Bug 6 (Schlep Blindness) — if you're tempted to skip this idea
           BECAUSE of the schlep, that's exactly why it might be good.
```

### S2: Boring Space (无聊领域)

```
+1 if: The industry/problem sounds boring — no one brags about working on it.
       Insurance, compliance, logistics, procurement, HR, accounting, PCB manufacturing...
Why positive: Smart people avoid boring problems → less competition → more opportunity.
```

### S3: Existing Competitors (已有竞品)

```
+1 if: There are existing players but they're complacent, outdated, or have
       obvious gaps you can exploit.
Why positive: Competitors = validated market. Beating them is easier than
              creating a market from scratch.
```

---

## Phase 4: Schlep Blindness Reverse Check (Schlep盲区逆向检测)

After scoring, run this additional check:

```yaml
question: >
  Did you skip or dismiss any directions BEFORE arriving at this idea?
  If yes: what was the reason you dismissed them?

schlep_detection:
  If dismissed reasons include:
    - "Too much non-coding work"
    - "Would need to talk to too many people"
    - "Not technical enough"
    - "Too much industry-specific knowledge needed"
    - "Would need to do sales/BD"
  Then: ⚠️ Schlep blindness may have filtered out better ideas.
  Suggest: Re-evaluate dismissed directions using the "investor test" —
           "If someone else was doing this, would you invest?"
```

---

## Output Format

```
## 想法质检报告 (Idea Evaluation Report)

**想法概述**: [one sentence]

### 排雷扫描
- M1 SISP: ✅ Clear / ⚠️ Detected — [detail]
- M2 Tar Pit: ✅ Clear / ⚠️ Detected — [structural reason]
- M3 Homework: ✅ Done / ⚠️ Not done — [what's missing]
- M4 Idea Shopping: ✅ Clear / ⚠️ Detected

### 十问评分

| # | 维度 | 分数 | 依据 |
|---|------|------|------|
| Q1 | Founder-Market Fit | X/2 | [one line] |
| Q2 | Market Size | X/2 | [one line] |
| Q3 | Problem Acuteness | X/2 | [one line] |
| Q4 | Competition | X/2 | [one line] |
| Q5 | Personal Desire | X/2 | [one line] |
| Q6 | Timing | X/2 | [one line] |
| Q7 | Proxy | X/2 | [one line] |
| Q8 | Commitment | X/2 | [one line] |
| Q9 | Scalability | X/2 | [one line] |
| Q10 | Idea Space | X/2 | [one line] |

**基础分**: X/20

### 反直觉加分
- S1 启动困难: +1 / +0 — [reason]
- S2 无聊领域: +1 / +0 — [reason]
- S3 已有竞品: +1 / +0 — [reason]

**调整后总分**: (基础分 + 加分) / 2.3 = **X/10**

### Schlep盲区逆向检测
[结果]

### 判定

🟢 ≥6/10 — 值得进入六模式审评
🟡 3-5/10 — 需改进，具体建议：[list weak dimensions + how to improve]
🔴 ≤2/10 — 建议放弃或彻底重构方向

### 下一步
[Specific action based on score — either "proceed to six-model review"
 or "do X to improve before re-evaluating" or "drop and try next direction"]
```

---

## Usage Notes

- This skill is designed for rapid evaluation — a single idea should take 5-10 minutes to score.
- Multiple ideas can be scored in parallel for comparison. When comparing, present a summary table at the end.
- The score is a screening tool, not a verdict. A 4/10 idea with one fixable weakness might be better than a 7/10 idea you're not excited about.
- If the user is comparing multiple directions, run this on each and present a comparison matrix.
- After scoring ≥6, hand off to the six-model review (starting with Model 04 Fix the Roof).
```

Create `packages/agent-core/src/skill/builtin/idea-evaluator.ts`:

```typescript
import { parseSkillText } from '../parser';
import type { SkillDefinition } from '../types';
import IDEA_EVALUATOR_BODY from './idea-evaluator.md';

const PSEUDO_PATH = 'builtin://idea-evaluator';

const parsed = parseSkillText({
  skillMdPath: '/builtin/skills/idea-evaluator.md',
  skillDirName: 'idea-evaluator',
  source: 'builtin',
  text: IDEA_EVALUATOR_BODY,
});

export const IDEA_EVALUATOR_SKILL: SkillDefinition = {
  ...parsed,
  path: PSEUDO_PATH,
  dir: PSEUDO_PATH,
  metadata: {
    ...parsed.metadata,
    type: parsed.metadata.type ?? 'inline',
  },
};
```

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected: tests pass with 15 built-in skills.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/skill/builtin/idea-evaluator.md packages/agent-core/src/skill/builtin/idea-evaluator.ts packages/agent-core/test/skill/builtin-skills.test.ts
git commit -m "feat(agent-core): add idea-evaluator built-in skill"
```

---

### Task B6: Register both skills in the built-in registry

**Depends on:** Task B4, Task B5

**Files:**
- Modify: `packages/agent-core/src/skill/builtin/index.ts`
- Modify: `packages/agent-core/test/skill/builtin-skills.test.ts` (add hiddenInModes assertions)

This task wires the skill constants into the registry so they are loaded at runtime. It also adds the `hiddenInModes` assertions required by the design.

- [ ] **Write the failing test.** Update `packages/agent-core/test/skill/builtin-skills.test.ts` to add the `hiddenInModes` check inside the existing `it.each` block:

```typescript
  it.each(BUILTIN_SKILLS)('skill "$name" has correct metadata', ({ skill, name }) => {
    expect(skill.name).toBe(name);
    expect(skill.source).toBe('builtin');
    expect(skill.path).toBe(`builtin://${name}`);
    expect(skill.dir).toBe(`builtin://${name}`);
    expect(skill.content.length).toBeGreaterThan(0);
    expect(skill.description.length).toBeGreaterThan(0);

    if (name === 'idea-generator' || name === 'idea-evaluator') {
      expect(skill.metadata.hiddenInModes).toEqual(
        expect.arrayContaining(['plan', 'design', 'office-hours', 'game-design']),
      );
    }
  });
```

- [ ] **Run it and verify it FAILS.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected failure: the test passes (the metadata is already correct from B4/B5), but the registry does not yet export the skills, so this task's real verification is the registration step. Alternatively, if the registry export was not added, the import may fail. Treat the registration change as the implementation.

- [ ] **Write the minimal implementation.** Modify `packages/agent-core/src/skill/builtin/index.ts`:

```typescript
import type { SkillRegistry } from '../registry';
import { DISPATCHING_PARALLEL_AGENTS_SKILL } from './dispatching-parallel-agents';
import { EXECUTING_PLANS_SKILL } from './executing-plans';
import { FINISHING_A_DEVELOPMENT_BRANCH_SKILL } from './finishing-a-development-branch';
import { IDEA_EVALUATOR_SKILL } from './idea-evaluator';
import { IDEA_GENERATOR_SKILL } from './idea-generator';
import { MCP_CONFIG_SKILL } from './mcp-config';
import { RECEIVING_CODE_REVIEW_SKILL } from './receiving-code-review';
import { REQUESTING_CODE_REVIEW_SKILL } from './requesting-code-review';
import { SIMPLICITY_FIRST_SKILL } from './simplicity-first';
import { SUBAGENT_DRIVEN_DEVELOPMENT_SKILL } from './subagent-driven-development';
import { SYNC_CHANGELOG_SKILL } from './sync-changelog';
import { SYSTEMATIC_DEBUGGING_SKILL } from './systematic-debugging';
import { TEST_DRIVEN_DEVELOPMENT_SKILL } from './test-driven-development';
import { USING_GIT_WORKTREES_SKILL } from './using-git-worktrees';
import { VERIFICATION_BEFORE_COMPLETION_SKILL } from './verification-before-completion';
import { DEBT_LEDGER_SKILL } from './debt-ledger';
import { registerGameDesignSkills } from './game-design-skills';

export function registerBuiltinSkills(registry: SkillRegistry): void {
  registry.registerBuiltinSkill(DISPATCHING_PARALLEL_AGENTS_SKILL);
  registry.registerBuiltinSkill(EXECUTING_PLANS_SKILL);
  registry.registerBuiltinSkill(FINISHING_A_DEVELOPMENT_BRANCH_SKILL);
  registry.registerBuiltinSkill(IDEA_EVALUATOR_SKILL);
  registry.registerBuiltinSkill(IDEA_GENERATOR_SKILL);
  registry.registerBuiltinSkill(MCP_CONFIG_SKILL);
  registry.registerBuiltinSkill(RECEIVING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(REQUESTING_CODE_REVIEW_SKILL);
  registry.registerBuiltinSkill(SIMPLICITY_FIRST_SKILL);
  registry.registerBuiltinSkill(SUBAGENT_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(SYNC_CHANGELOG_SKILL);
  registry.registerBuiltinSkill(SYSTEMATIC_DEBUGGING_SKILL);
  registry.registerBuiltinSkill(TEST_DRIVEN_DEVELOPMENT_SKILL);
  registry.registerBuiltinSkill(USING_GIT_WORKTREES_SKILL);
  registry.registerBuiltinSkill(VERIFICATION_BEFORE_COMPLETION_SKILL);
  registry.registerBuiltinSkill(DEBT_LEDGER_SKILL);
  registerGameDesignSkills(registry);
}

export {
  DISPATCHING_PARALLEL_AGENTS_SKILL,
  EXECUTING_PLANS_SKILL,
  FINISHING_A_DEVELOPMENT_BRANCH_SKILL,
  IDEA_EVALUATOR_SKILL,
  IDEA_GENERATOR_SKILL,
  MCP_CONFIG_SKILL,
  RECEIVING_CODE_REVIEW_SKILL,
  REQUESTING_CODE_REVIEW_SKILL,
  SIMPLICITY_FIRST_SKILL,
  SUBAGENT_DRIVEN_DEVELOPMENT_SKILL,
  SYNC_CHANGELOG_SKILL,
  SYSTEMATIC_DEBUGGING_SKILL,
  TEST_DRIVEN_DEVELOPMENT_SKILL,
  USING_GIT_WORKTREES_SKILL,
  VERIFICATION_BEFORE_COMPLETION_SKILL,
  DEBT_LEDGER_SKILL,
};
```

- [ ] **Run it and verify it PASSES.**

```bash
pnpm test packages/agent-core/test/skill/builtin-skills.test.ts
```

Expected: all tests pass, including the new `hiddenInModes` assertions.

- [ ] **Commit.**

```bash
git add packages/agent-core/src/skill/builtin/index.ts packages/agent-core/test/skill/builtin-skills.test.ts
git commit -m "feat(agent-core): register idea skills and assert hiddenInModes"
```

---

## Local Self-Review (Phase B)

- [ ] **Spec coverage:** Both idea skills are added with correct frontmatter, upstream content is preserved, and `hiddenInModes` hides them in the four non-normal modes.
- [ ] **No placeholders:** Full frontmatter, wrapper code, and test code are provided. No TODO/TBD.
- [ ] **No phantom tasks:** Each task creates files and ends with a passing test + commit.
- [ ] **Dependency soundness:** B5 depends on B4 (test file is extended). B6 depends on B4 and B5. No later symbols are referenced.
- [ ] **Caller & build soundness:** B6 modifies the shared `skill/builtin/index.ts` export list and registry registration. End Phase B with a single-package typecheck:

```bash
pnpm --filter @odysseythink/agent-core typecheck
```

- [ ] **Test-the-risk:**
  - Skill count increments from 13 → 14 → 15, preventing dropped skills.
  - `hiddenInModes` explicitly asserts all four non-normal modes.
  - Alphabetical sorting test ensures registry order remains deterministic.
- [ ] **Type consistency:** Skill wrappers use the same `SkillDefinition` shape as existing built-in skills. Exports are added consistently.
