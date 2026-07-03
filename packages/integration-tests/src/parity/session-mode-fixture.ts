export interface SessionModeFixture {
  description: string;
  steps: SessionModeStep[];
  expectedEvents: Array<Record<string, unknown>>;
}

export type SessionModeStep =
  | { action: 'enter'; kind: 'plan' | 'design' | 'office-hours' | 'game-design'; id?: string }
  | { action: 'exit'; id?: string }
  | { action: 'cancel'; id?: string }
  | { action: 'handoff'; target: 'plan' | 'normal' }
  | { action: 'inject' }
  | { action: 'assert'; isActive: boolean; kind?: string | null };
