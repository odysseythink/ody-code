/**
 * Checkpoint integrity verification.
 *
 * Verifies that a checkpoint payload is structurally sound and internally
 * consistent before it is promoted to the recovery index. The verifier is
 * intentionally defensive: it returns a result object with per-check flags
 * and human-readable errors instead of throwing, so callers can decide
 * whether to skip a version and fall back to an earlier one.
 */

import type { Message, ToolCall } from '@odysseythink/kosong';

import { RUNTIME_MODES } from '../../agent/session-mode';
import type { RuntimeMode } from '../../agent/session-mode';

import type { SessionCheckpointPayload } from './checkpoint';

export interface CheckpointIntegrityOptions {
  /** Expected number of messages in the source of truth (e.g. live memory). */
  readonly expectedMessageCount?: number | undefined;
  /** Expected session id; mismatch fails the check. */
  readonly expectedSessionID?: string | undefined;
}

export interface CheckpointIntegrityChecks {
  /** Payload is a valid object with the required top-level fields. */
  jsonValid: boolean;
  /** Message count matches the expected value, or null when no expectation was given. */
  messageCountMatch: boolean | null;
  /** Session ID matches the expected value, or null when no expectation was given. */
  sessionIDMatch: boolean | null;
  /** Design session references point to valid message indices. */
  designModeConsistent: boolean;
  /** Every tool result has a matching tool call and vice-versa. */
  toolCallIndexComplete: boolean;
}

export interface CheckpointIntegrityResult {
  /** True only when every executed check passed. */
  readonly valid: boolean;
  /** Per-check outcomes. */
  readonly checks: CheckpointIntegrityChecks;
  /** Human-readable failure reasons; empty when valid. */
  readonly errors: string[];
}

/**
 * Validate a checkpoint payload for structural and semantic integrity.
 */
export function verifyCheckpointIntegrity(
  payload: unknown,
  options: CheckpointIntegrityOptions = {},
): CheckpointIntegrityResult {
  const errors: string[] = [];

  const jsonValid = validateJson(payload, errors);
  const messageCountMatch =
    options.expectedMessageCount === undefined
      ? null
      : validateMessageCount(payload, options.expectedMessageCount, errors);
  const designModeConsistent = jsonValid
    ? validateDesignMode(payload as SessionCheckpointPayload, errors)
    : false;
  const toolCallIndexComplete = jsonValid
    ? validateToolCallIndex(payload as SessionCheckpointPayload, errors)
    : false;

  const sessionIDMatch =
    options.expectedSessionID === undefined
      ? null
      : validateSessionID(payload, jsonValid, options.expectedSessionID, errors);

  const checks: CheckpointIntegrityChecks = {
    jsonValid,
    messageCountMatch,
    sessionIDMatch,
    designModeConsistent,
    toolCallIndexComplete,
  };

  const valid =
    jsonValid &&
    designModeConsistent &&
    toolCallIndexComplete &&
    (messageCountMatch === null || messageCountMatch) &&
    (sessionIDMatch === null || sessionIDMatch);

  return { valid, checks, errors };
}

function validateJson(payload: unknown, errors: string[]): boolean {
  if (payload === null || typeof payload !== 'object') {
    errors.push('Checkpoint payload is not an object');
    return false;
  }

  const typed = payload as Partial<SessionCheckpointPayload>;
  let ok = true;

  if (typeof typed.sessionID !== 'string' || typed.sessionID.length === 0) {
    errors.push('Missing or invalid sessionID');
    ok = false;
  }

  if (typeof typed.createdAt !== 'string' || typed.createdAt.length === 0) {
    errors.push('Missing or invalid createdAt');
    ok = false;
  }

  if (typeof typed.lastUpdatedAt !== 'string' || typed.lastUpdatedAt.length === 0) {
    errors.push('Missing or invalid lastUpdatedAt');
    ok = false;
  }

  const currentMode = typed.currentMode as string | undefined;
  if (!RUNTIME_MODES.includes(currentMode as RuntimeMode)) {
    errors.push(`Invalid currentMode: ${String(currentMode)}`);
    ok = false;
  }

  if (!Array.isArray(typed.messages)) {
    errors.push('Missing or invalid messages array');
    ok = false;
  }

  const sessions = typed.designModeContext?.sessions;
  if (!Array.isArray(sessions)) {
    errors.push('Missing or invalid designModeContext.sessions array');
    ok = false;
  }

  const callIdToResult = typed.toolCallIndex?.callIdToResult;
  if (callIdToResult === null || typeof callIdToResult !== 'object') {
    errors.push('Missing or invalid toolCallIndex.callIdToResult');
    ok = false;
  }

  return ok;
}

function validateSessionID(
  payload: unknown,
  jsonValid: boolean,
  expected: string,
  errors: string[],
): boolean {
  if (!jsonValid) return false;
  const typed = payload as SessionCheckpointPayload;
  if (typed.sessionID !== expected) {
    errors.push(`Session ID mismatch: expected ${expected}, got ${typed.sessionID}`);
    return false;
  }
  return true;
}

function validateMessageCount(
  payload: unknown,
  expected: number,
  errors: string[],
): boolean {
  const typed = payload as SessionCheckpointPayload;
  const actual = typed.messages.length;
  if (actual !== expected) {
    errors.push(`Message count mismatch: expected ${expected}, got ${actual}`);
    return false;
  }
  return true;
}

function validateDesignMode(payload: SessionCheckpointPayload, errors: string[]): boolean {
  const messages = payload.messages;
  const sessions = payload.designModeContext.sessions;
  const seenIds = new Set<string>();
  let ok = true;

  for (let i = 0; i < sessions.length; i++) {
    const session = sessions[i];
    if (session === undefined) continue;

    if (typeof session.designSessionID !== 'string' || session.designSessionID.length === 0) {
      errors.push(`designModeContext.sessions[${i}] has invalid designSessionID`);
      ok = false;
      continue;
    }

    if (seenIds.has(session.designSessionID)) {
      errors.push(`Duplicate designSessionID: ${session.designSessionID}`);
      ok = false;
    }
    seenIds.add(session.designSessionID);

    // startedAtMsg / exitedAtMsg are boundary cursors (count of preceding
    // messages), not element indices, so the valid range is the INCLUSIVE
    // [0, messages.length]: a session entered (or exited) after the last message
    // has a cursor equal to the length. An exclusive upper bound here produced a
    // spurious "out of range [0, N)" warning on resume for sessions started at
    // the very end of history.
    const started = session.startedAtMsg;
    if (!Number.isInteger(started) || started < 0 || started > messages.length) {
      errors.push(
        `Design session ${session.designSessionID} startedAtMsg ${started} is out of range [0, ${messages.length}]`,
      );
      ok = false;
      continue;
    }

    if (session.exitedAtMsg !== undefined) {
      const exited = session.exitedAtMsg;
      if (!Number.isInteger(exited) || exited < started || exited > messages.length) {
        errors.push(
          `Design session ${session.designSessionID} exitedAtMsg ${exited} is invalid (must be >= ${started} and <= ${messages.length})`,
        );
        ok = false;
      }
    }
  }

  return ok;
}

function validateToolCallIndex(payload: SessionCheckpointPayload, errors: string[]): boolean {
  const messages = payload.messages as Message[];
  const callIds = new Set<string>();
  const resultIds = new Set<string>();

  for (const message of messages) {
    if (message.role === 'assistant' && Array.isArray(message.toolCalls)) {
      for (const toolCall of message.toolCalls as ToolCall[]) {
        if (toolCall.id !== undefined && toolCall.id.length > 0) {
          callIds.add(toolCall.id);
        }
      }
    }

    if (message.role === 'tool' && typeof message.toolCallId === 'string') {
      resultIds.add(message.toolCallId);
    }
  }

  let ok = true;

  for (const resultId of resultIds) {
    if (!callIds.has(resultId)) {
      errors.push(`Tool result ${resultId} has no matching assistant tool call`);
      ok = false;
    }
  }

  for (const callId of callIds) {
    if (!resultIds.has(callId)) {
      errors.push(`Tool call ${callId} has no matching tool result message`);
      ok = false;
    }
  }

  return ok;
}
