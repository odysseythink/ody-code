import type { Agent, ModeKey } from '..';
import type { AgentReplayRecord } from '../..';
import type { ContextMessage } from '../context';

export class ReplayBuilder {
  protected readonly records: AgentReplayRecord[] = [];
  private _mode: ModeKey = 'normal';

  constructor(public readonly agent: Agent) {}

  setMode(mode: ModeKey): void {
    this._mode = mode;
  }

  push(record: AgentReplayRecord): void {
    if (this.agent.records.restoring) {
      const tagged: AgentReplayRecord =
        record.type === 'message' ? { ...record, mode: this._mode } : record;
      this.records.push(tagged);
    }
  }

  removeLastMessages(removedMessages: ReadonlySet<ContextMessage>): void {
    if (removedMessages.size === 0) return;
    for (let i = this.records.length - 1; i >= 0; i--) {
      const record = this.records[i]!;
      if (record.type === 'message' && removedMessages.has(record.message)) {
        this.records.splice(i, 1);
      }
    }
  }

  buildResult(): readonly AgentReplayRecord[] {
    return this.records;
  }

  buildResultForMode(mode: ModeKey): readonly AgentReplayRecord[] {
    return this.records.filter((r) => {
      if (r.type !== 'message') return true;
      return r.mode === mode;
    });
  }
}
