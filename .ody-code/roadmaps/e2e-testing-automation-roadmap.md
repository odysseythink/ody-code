# E2E Testing Automation Roadmap

**Document Type**: Product Roadmap  
**Last Updated**: 2026-06-16  
**Status**: Phase 1 Implemented, Phase 2 Ready  
**Epic Owner**: TBD  

---

## 📋 Executive Summary

**Objective**: Implement AI-driven E2E test generation and execution as a standard capability within ody-code's WritingPlan and ExecutingPlan workflows.

**Value Proposition**:
- ✅ When AI generates code, automatically generate corresponding E2E tests
- ✅ Verify new functionality correctness
- ✅ Protect against regressions in existing functionality
- ✅ Provide closed-loop feedback: generate → test → report
- ✅ Extensible framework supporting multiple tech stacks (TS, Python, Go, Java, etc.)

**Timeline**: 3-6 months  
**Effort**: 1-2 FTE  
**Priority**: High (Phase 1), Medium (Phase 2+)

---

## 🎯 Vision & Scope

### Current State (As-Is)
- AI generates code through Design Mode → Plan Mode → Execution Mode
- Single unit tests for new features (limited scope)
- No integration/E2E testing automation
- Manual test verification by user

### Future State (To-Be)
- **Phase 1**: E2E testing for ody-code itself (TS/Vitest)
- **Phase 2**: Multi-language support (Python/Node.js/Go)
- **Phase 3**: Intelligent decision-making (ML-driven, contract testing, mutation testing)

### Scope by Phase

| Phase | Scope | Tech Stack | Duration | Value | Status |
|-------|-------|-----------|----------|-------|--------|
| **Phase 1** | ody-code self-testing framework | TS + Vitest | 2-3 weeks | Foundation | ✅ Implemented |
| **Phase 2** | Multi-language generators | Go (done), Python, Node.js | 4-6 weeks | User value | 🚧 In Progress (Go landed) |
| **Phase 3** | Intelligent optimization | ML, contract testing | 6+ weeks | Enterprise grade | 📅 Future |

---

## 📊 Phase 1: Framework & Self-Validation (2-3 weeks)

### Goals
- Build extensible E2E test generation framework
- Integrate with WritingPlan (auto-add E2E tasks)
- Integrate with ExecutingPlan (auto-generate + run tests)
- Configuration-driven capability toggle
- Dog-food the capability on ody-code itself

### Deliverables

Implemented files (names differ slightly from the original plan):

```
packages/agent-core/src/e2e-testing/
├── types.ts                    # type definitions
├── registry.ts                 # generator registry
├── config.ts                   # config resolution
├── impact-analyzer.ts          # affected-scope detection
├── impact-map.ts               # static tool → file mapping
├── executor.ts                 # test executor
├── generator.ts                # TypeScript/Vitest generator
├── plan-enricher.ts            # plan-mode E2E task injection
├── git-status.ts               # git status parser
└── errors.ts                   # error types

packages/agent-core/test/e2e-testing/
├── core.test.ts                # config + registry + impact analysis
├── generator.test.ts           # TypeScript/Vitest generator
├── executor.test.ts            # executor
├── integration.test.ts         # RunE2ETests + checkpoint hooks
└── plan-enrichment.e2e.test.ts # ExitPlanMode enrichment end-to-end

.ody-code/
├── docs/e2e-testing-guide.md   # user guide
└── roadmaps/
    └── this file
```

> **Note**: `.ody-code/e2e-config.yaml` was not created. E2E configuration lives in the existing `[e2e]` section of `~/.ody-code/config.toml` (or workspace `config.toml`), parsed by `packages/agent-core/src/config/schema.ts`.

### Key Features
1. **E2E Generator Interface** (extensible)
   - `E2ETestGenerator`: abstract interface for multi-language support
   - `E2EGeneratorRegistry`: factory pattern for generator selection
   - Supports: TS/Vitest (Phase 1), Python/Pytest (Phase 2), Node/Jest (Phase 2)

2. **Configuration** (`.ody-code/e2e-config.yaml`)
   - `enabled`: boolean toggle
   - `strategy`: 'always' | 'smart' | 'critical-only'
   - `criticalTools`: list of tools that must have E2E
   - `failurePolicy`: how to handle test failures

3. **WritingPlan Integration**
   - Auto-detect modified files
   - Analyze impact scope (which existing tools are affected)
   - Automatically add E2E test task if needed
   - Tag tasks with priority: critical/important/nice-to-have

4. **ExecutingPlan Integration**
   - Auto-generate E2E test files from templates
   - Parallel execution with configurable concurrency
   - Generate summary report
   - Fail fast on critical failures (configurable)

### Success Criteria
- [x] All code passes `tsc --noEmit` — verified during Phase 1 implementation
- [x] Unit tests: 49 tests across 5 test files, all passing
- [x] Dog-food validation: ExitPlanModeTool E2E enrichment works end-to-end (`plan-enrichment.e2e.test.ts`)
- [x] Configuration loads and applies correctly (`[e2e]` section in `config.toml`)
- [x] Documentation complete (`.ody-code/docs/e2e-testing-guide.md`)

### Timeline
| Week | Tasks | Status |
|------|-------|--------|
| 1.1-1.2 | Architecture design + framework scaffolding | ✅ Complete |
| 1.2-1.3 | TypeScript/Vitest generator implementation | ✅ Complete |
| 1.3-1.4 | WritingPlan & ExecutingPlan integration | ✅ Complete |
| 1.4-1.5 | Testing, dog-fooding, documentation | ✅ Complete |

### Phase 1 Completion Notes
- Design doc: `.ody-code/designs/2026-06-16-e2e-testing-automation-phase-1.md`
- Plan doc: `.ody-code/plans/2026-06-16-e2e-testing-automation-phase-1.md`
- Changeset: `.changeset/feat-e2e-testing-automation-phase-1.md`
- All 49 E2E-framework unit tests pass.

### Risk Mitigation
| Risk | Mitigation |
|------|-----------|
| Test generation quality | Use strict templates + code review |
| Execution too slow | Parallel + caching + smart selection |
| Complex to integrate | Start with simple WritingPlan/ExecutingPlan hooks |

---

## 📊 Phase 2: Multi-Language Support (4-6 weeks)

### Goals
- Detect target project tech stack automatically
- Generate E2E tests for Python projects (Pytest)
- Generate E2E tests for Node.js projects (Jest)
- Improve impact analysis (transitive dependencies)
- Optimize parallel execution and caching

### Deliverables
- ✅ `generators/go.ts` — Go generator (HTTP-server / CLI / generic templates, real
  subprocess black-box e2e, `go test -json` parsing). **Replaces Python as the first
  Phase 2 language** (most user projects are Go).
- ✅ Language-pluggable execution: `runTests` + `analyzeImpact` moved onto the
  `E2ETestGenerator` interface; `E2ETestExecutor` is now a language-agnostic
  orchestrator (no longer hardcoded to Vitest).
- `generators/python-pytest.ts` (deferred)
- `generators/nodejs-jest.ts` (deferred)
- Enhanced `impact-analysis.ts` (recursive dependency graph) (deferred)
- Caching layer for test results (deferred)

### New Capabilities
1. **Tech Stack Detection**
   - Analyze package.json, pyproject.toml, go.mod, etc.
   - Return `ProjectStructure` with language, framework, test tool

2. **Python Pytest Generator**
   - Template for pytest-style E2E tests
   - Run via `pytest` CLI
   - Parse output for results

3. **Node.js Jest Generator**
   - Template for Jest-style E2E tests
   - Run via `jest` CLI
   - Support both CommonJS and ESM

4. **Smarter Impact Analysis**
   - Recursive dependency traversal
   - Identify transitive dependencies
   - Mark each affected tool: critical/important/nice-to-have

### Success Criteria
- [x] Go projects auto-detect (`go.mod`) and generate `go test` E2E that builds,
      spawns the server and asserts on the HTTP JSON response (verified end-to-end
      against a real `net/http` server with Go 1.26)
- [ ] Python projects auto-detect and generate pytest E2E
- [ ] Node.js projects auto-detect and generate jest E2E
- [ ] Impact analysis handles 3+ levels of dependencies
- [ ] E2E execution time < 15s for typical change

### Timeline
| Week | Tasks |
|------|-------|
| 2.1-2.3 | Python/Pytest generator |
| 2.3-2.5 | Node.js/Jest generator + impact analysis enhancement |
| 2.5-2.6 | Integration, testing, documentation |

---

## 📊 Phase 3: Intelligent Optimization (6+ weeks, continuous)

### Goals (Future Exploration)
- ML-driven priority decision-making
- Contract testing (lightweight alternative to full E2E)
- Mutation testing (ensure test quality)
- Historical data tracking and trend analysis

### Potential Features
1. **Risk Scoring**
   - ML model predicts bug probability based on code complexity
   - Automatically adjust E2E scope based on risk

2. **Contract Testing**
   - Verify interface contracts between tools
   - Faster than full E2E
   - Catch integration bugs efficiently

3. **Mutation Testing**
   - Inject code mutations
   - Verify generated tests can detect mutations
   - Identify weak tests

4. **Caching & Optimization**
   - Cache immutable dependencies
   - Reuse test results for unchanged code
   - Distribued test execution (future)

---

## 🔄 How It Works (Workflow)

### WritingPlan Phase
```
User Request: "Add validation for design completeness"
  ↓
AI generates Design (Design Mode)
  ↓
WritingPlan triggered:
  1. Analyze modified files
  2. Detect affected tools (e.g., ExitDesignModeTool, SessionMode)
  3. Check if E2E needed (configured strategy)
  4. Auto-add task: "Generate E2E tests for affected tools"
  ↓
Plan presented to user
  ↓
User approves plan
```

### ExecutingPlan Phase
```
ExecutingPlan starts:
  ↓
Execute implementation tasks
  ↓
Reach "Generate E2E tests" task:
  1. Detect project tech stack
  2. Get appropriate generator (TypeScript/Vitest)
  3. Generate E2E test files from templates
  4. Write to disk (test/e2e/validation-tool.test.ts)
  ↓
Execute "Run E2E tests" task:
  1. Run in parallel (max 4 concurrent)
  2. Parse results
  3. Generate report
  ↓
Critical tests fail?
  → BLOCK (failure policy)
  → Report and ask user to fix
  ↓
Important tests fail?
  → WARN (failure policy)
  → Continue but flag
  ↓
All pass?
  → ✅ Success
  → Proceed to next phase or deliver
```

---

## 📈 Metrics & KPIs

### Phase 1 Success Metrics
- E2E test auto-generation coverage: target 100% (for ody-code changes)
- Test execution time: < 10s (goal)
- Test pass rate: >= 95%
- Code quality: 0 type errors, >= 80% coverage

### Phase 2+ Metrics
- Support for 3+ tech stacks
- User adoption rate (% of projects with auto E2E)
- Time saved per project (vs manual E2E writing)
- Bugs caught by auto E2E (vs manual testing)

---

## 🛠️ Technical Architecture

### Core Abstractions

```typescript
// Base interface for extensibility
interface E2ETestGenerator {
  readonly id: string;
  detectProjectStructure(root: string): Promise<ProjectStructure>;
  generateTestsForFeature(feature: Feature): Promise<TestFile[]>;
  runTests(testDir: string, options?): Promise<TestResult>;
}

// Registry for multi-language support
class E2EGeneratorRegistry {
  register(id: string, generator: E2ETestGenerator): void;
  detectAndGet(projectRoot: string): Promise<E2ETestGenerator>;
}

// Impact analysis
function analyzeCodeImpact(files: string[], critical: string[]): ImpactAnalysisResult;

// Execution
class E2ETestExecutor {
  executeE2ETests(features: Feature[], projectRoot: string): Promise<E2EExecutionResult>;
}
```

### Integration Points
1. **WritingPlan**: Hook to analyze impact and add E2E task
2. **ExecutingPlan**: Hook to execute E2E task
3. **Configuration**: `.ody-code/e2e-config.yaml`
4. **Reporting**: `.ody-code/test-reports/`

---

## 📋 Dependencies & Prerequisites

### Required
- Node.js 20+
- TypeScript 5+
- Vitest (Phase 1)
- Python 3.9+ (Phase 2)
- Jest (Phase 2)

### Optional (Future)
- Stryker (mutation testing)
- Machine learning library (risk scoring)

---

## 🎓 Learning & Feedback Points

### Phase 1 Retrospective (end of Week 1.5)
Questions to answer:
1. How good is the auto-generated test quality? (use mutation testing to measure)
2. How accurate is the impact analysis? (false positive/negative rates)
3. Is execution time acceptable? (target < 10s)
4. What breaks during dog-fooding?

### Phase 2 Planning (based on Phase 1 feedback)
- Which languages to prioritize? (based on user demand)
- How to improve test generation quality?
- Optimize caching and parallel execution?

---

## 📞 Stakeholders & Ownership

| Role | Responsibility |
|------|-----------------|
| **Product Owner** | Vision, roadmap, prioritization |
| **Tech Lead** | Architecture, design review |
| **Backend Engineer** | Implementation (2 FTE) |
| **Test Engineer** | Test strategy, quality metrics |
| **Documentation** | User guides, API docs |

---

## 📅 Milestones

### Milestone 1: Phase 1 Complete (Week 1.5) ✅
- Framework scaffolded and tested
- TypeScript/Vitest generator working
- WritingPlan + ExecutingPlan integrated
- Dog-fooding validation passed (ExitPlanModeTool E2E)
- Merged to main

**Gate Decision**: Proceed to Phase 2? **YES** — Phase 1 metrics met, ready to start multi-language support.

### Milestone 2: Phase 2 Complete (Week 2.5)
- Multi-language support (Python, Node.js)
- Enhanced impact analysis
- Performance optimization
- PR ready for review

**Gate Decision**: Release to users?
- If adoption metrics high: YES → General availability
- If issues: NO → Phase 2.1 (bugfixes)

### Milestone 3: Phase 3 (Ongoing, 6+ weeks)
- ML-driven optimization
- Contract testing
- Mutation testing
- Continuous improvement loop

---

## 🚀 Next Steps

1. ~~Approve this roadmap~~ ✅ Done
2. ~~Create Phase 1 detailed tasks~~ ✅ Done
3. ~~Assign Phase 1 owner~~ ✅ Done
4. **Start Phase 2 tasks** — multi-language generators (Python/Pytest, Node.js/Jest)
5. **Weekly sync** (every Friday to review Phase 2 progress)

### Phase 2 Entry Checklist
- [ ] Define generator interface changes needed for multi-language support
- [ ] Add tech-stack detection for `pyproject.toml`, `requirements.txt`, `package.json` (Jest)
- [ ] Implement Python/Pytest generator
- [ ] Implement Node.js/Jest generator
- [ ] Enhance `ImpactAnalyzer` with recursive dependency traversal
- [ ] Add test-result caching layer

---

## 📖 Related Documents

- **Configuration**: `[e2e]` section in `~/.ody-code/config.toml` (schema in `packages/agent-core/src/config/schema.ts`)
- **User Guide** (Phase 1): `.ody-code/docs/e2e-testing-guide.md` ✅
- **Design Doc** (Phase 1): `.ody-code/designs/2026-06-16-e2e-testing-automation-phase-1.md`
- **Plan Doc** (Phase 1): `.ody-code/plans/2026-06-16-e2e-testing-automation-phase-1.md`
- **API Docs**: `packages/agent-core/src/e2e-testing/README.md` (to be created in Phase 2 or as follow-up)

---

## 📝 Appendix: FAQ

### Q: Why start with ody-code self-testing (Phase 1)?
**A**: Dog-fooding validates the capability before extending to users. Also, it protects ody-code's own quality during development.

### Q: Why not do all languages at once?
**A**: Multi-language support is complex (AST parsing, build tools, testing frameworks differ). Better to ship TS/Vitest first, then iterate on others.

### Q: What about existing projects?
**A**: Phase 1 only works for new ody-code changes. Phase 2 will support user projects. No breaking changes to existing projects.

### Q: How long is each test?
**A**: Target < 5s per E2E test. If slower, optimize or split.

### Q: Can I disable E2E for certain projects?
**A**: Yes, via `.ody-code/e2e-config.yaml`. Set `enabled: false` or use `critical-only` strategy.

---

**Last Updated**: 2026-06-16  
**Version**: 1.1  
**Status**: Phase 1 Implemented, Phase 2 Ready
