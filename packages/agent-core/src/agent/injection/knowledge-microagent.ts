import type { ContextMessage } from '../context';
import type { SkillDefinition } from '../../skill';
import { DynamicInjector } from './injector';
import type { Agent } from '..';
import { flags } from '../../flags';

// ── Types ──────────────────────────────────────────────────────────────

export interface MatchKnowledgeMicroagentsOptions {
  readonly messageText: string;
  readonly microagents: readonly SkillDefinition[];
  readonly alreadyInjected: ReadonlySet<string>;
}

export interface KnowledgeMicroagentMatch {
  readonly skill: SkillDefinition;
  readonly trigger: string;
}

// ── Matcher ────────────────────────────────────────────────────────────

const REGEX_META = /[.*+?^${}()|[\]\\]/g;

function escapeRegex(str: string): string {
  return str.replace(REGEX_META, '\\$&');
}

function isAsciiOnly(str: string): boolean {
  return /^[\x00-\x7F]*$/.test(str);
}

/**
 * Returns true when `trigger` matches `text` with case-insensitive,
 * word-boundary-sensitive (ASCII) or substring (CJK) semantics.
 */
export function triggerMatches(text: string, trigger: string): boolean {
  const normalizedTrigger = trigger.toLowerCase();

  if (isAsciiOnly(trigger)) {
    const pattern = new RegExp('\\b' + escapeRegex(normalizedTrigger) + '\\b', 'i');
    return pattern.test(text);
  }

  // CJK / mixed scripts: literal substring match.
  return text.toLowerCase().includes(normalizedTrigger);
}

// ── Message extraction ─────────────────────────────────────────────────

/** ContentPart types from kosong */
interface TextContentPart {
  type: 'text';
  text: string;
}

function isTextPart(part: unknown): part is TextContentPart {
  return (
    typeof part === 'object' &&
    part !== null &&
    'type' in part &&
    (part as TextContentPart).type === 'text'
  );
}

function concatenateTextParts(content: ContextMessage['content']): string {
  return content.filter(isTextPart).map((p) => p.text).join('');
}

/**
 * Scan history from end to start, returning the text of the latest
 * real user message (skipping injections and compaction summaries).
 * Returns undefined when no such message exists or its text is empty.
 */
export function extractLatestUserText(
  history: readonly ContextMessage[],
): string | undefined {
  for (let i = history.length - 1; i >= 0; i--) {
    const message = history[i]!;
    if (message.role !== 'user') continue;
    if (message.origin?.kind === 'injection') continue;
    if (message.origin?.kind === 'compaction_summary') continue;
    const text = concatenateTextParts(message.content);
    if (text.trim().length > 0) return text;
  }
  return undefined;
}

// ── Match entry point ──────────────────────────────────────────────────

/**
 * Returns the list of knowledge microagents whose triggers match the
 * message text and have not been injected yet.
 */
export function matchKnowledgeMicroagents(
  options: MatchKnowledgeMicroagentsOptions,
): readonly KnowledgeMicroagentMatch[] {
  const text = options.messageText.toLowerCase();
  const matches: KnowledgeMicroagentMatch[] = [];

  for (const microagent of options.microagents) {
    if (options.alreadyInjected.has(microagent.name)) continue;

    const triggers = microagent.metadata.triggers;
    if (!Array.isArray(triggers) || triggers.length === 0) continue;

    for (const trigger of triggers) {
      if (typeof trigger !== 'string') continue;
      if (triggerMatches(text, trigger)) {
        matches.push({ skill: microagent, trigger });
        break; // one match per microagent is sufficient
      }
    }
  }

  return matches;
}

// ── Injector ───────────────────────────────────────────────────────────

export const KNOWLEDGE_MICROAGENT_VARIANT = 'knowledge_microagent';

export class KnowledgeMicroagentInjector extends DynamicInjector {
  protected override readonly injectionVariant = KNOWLEDGE_MICROAGENT_VARIANT;
  private readonly injectedNames = new Set<string>();

  override onContextClear(): void {
    super.onContextClear();
    this.injectedNames.clear();
  }

  override onContextCompacted(compactedCount: number): void {
    super.onContextCompacted(compactedCount);
    this.injectedNames.clear();
  }

  protected override getInjection(): string | undefined {
    if (!flags.enabled('repo-knowledge')) return undefined;
    if (this.agent.sessionMode.isActive) return undefined;
    if (this.agent.skills === null) return undefined;

    const text = extractLatestUserText(this.agent.context.history);
    if (text === undefined || text.trim().length === 0) return undefined;

    const microagents = this.agent.skills.registry.listKnowledgeMicroagents();
    if (microagents.length === 0) return undefined;

    const matches = matchKnowledgeMicroagents({
      messageText: text,
      microagents,
      alreadyInjected: this.injectedNames,
    });
    if (matches.length === 0) return undefined;

    const bodies: string[] = [];
    for (const match of matches) {
      const body = match.skill.content.trim();
      if (body.length === 0) {
        this.agent.log.warn(`Microagent ${match.skill.name} has empty body; skipping`);
        continue;
      }
      this.injectedNames.add(match.skill.name);
      this.agent.telemetry.track('microagent_injected', {
        skill_name: match.skill.name,
        trigger: match.trigger,
        skill_source: match.skill.source,
      });
      bodies.push(`## ${match.skill.name}\n\n${body}`);
    }

    if (bodies.length === 0) return undefined;

    return [
      "The following repo-specific conventions are relevant to your current task.",
      "Apply them without mentioning them to the user unless asked.",
      "",
      bodies.join("\n\n---\n\n"),
    ].join("\n");
  }
}
