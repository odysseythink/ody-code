/**
 * Welcome panel shown at the top of the TUI.
 * Renders a round-bordered box with the logo, session, model, and version.
 */

import type { Component } from '@earendil-works/pi-tui';
import { truncateToWidth, visibleWidth } from '@earendil-works/pi-tui';
import chalk from 'chalk';

import { isRainbowDancing, renderDanceWelcomeHeader } from '#tui/easter-eggs/dance';
import type { ColorPalette } from '#tui/theme/colors';
import type { AppState } from '#tui/types';

/** Static logo frame borrowed from the ody-rs TUI welcome widget. */
const LOGO_FRAME = [
  '               ██       ██               ',
  '              ████     ████              ',
  '             █████     █████             ',
  '            █████       █████            ',
  '           █████         █████           ',
  '          █████           █████          ',
  '           █████         █████           ',
  '            █████       █████            ',
  '             █████     █████             ',
  '              ███       ███              ',
] as const;

function centerLine(line: string, width: number): string {
  const vis = visibleWidth(line);
  if (vis >= width) return line;
  const leftPad = Math.floor((width - vis) / 2);
  return ' '.repeat(leftPad) + line;
}

export class WelcomeComponent implements Component {
  private state: AppState;
  private colors: ColorPalette;

  constructor(state: AppState, colors: ColorPalette) {
    this.state = state;
    this.colors = colors;
  }

  invalidate(): void {}

  render(width: number): string[] {
    const primary = (s: string): string => chalk.hex(this.colors.primary)(s);
    const primaryBold = (s: string): string => chalk.bold.hex(this.colors.primary)(s);
    const innerWidth = Math.max(10, width - 4);
    const pad = '  ';

    const logoLines = LOGO_FRAME.map((row) => primary(centerLine(row, innerWidth)));

    const isLoggedOut = !this.state.model;
    const welcomeLine = truncateToWidth(
      primary('Welcome to ') + primaryBold('Ody') + primary(', command-line coding agent'),
      innerWidth,
      '…',
    );

    let renderedHeaderLines = [
      ...logoLines,
      '',
      primary(centerLine(welcomeLine, innerWidth)),
    ];
    if (isRainbowDancing()) {
      renderedHeaderLines = renderDanceWelcomeHeader(
        this.colors,
        LOGO_FRAME,
        innerWidth,
        isLoggedOut,
      );
    }

    const dim = chalk.hex(this.colors.textDim);
    const labelStyle = chalk.bold.hex(this.colors.textDim);
    const tagline = truncateToWidth(
      dim(isLoggedOut ? 'Run /login or /provider to get started.' : 'Send /help for help information.'),
      innerWidth,
      '…',
    );

    const activeModel = this.state.availableModels[this.state.model];
    const modelValue = isLoggedOut
      ? chalk.hex(this.colors.warning)('not set, run /login or /provider')
      : (activeModel?.displayName ?? activeModel?.model ?? this.state.model);

    const infoLines = [
      labelStyle('Directory: ') + this.state.workDir,
      labelStyle('Session:   ') + this.state.sessionId,
      labelStyle('Model:     ') + modelValue,
      labelStyle('Version:   ') + this.state.version,
    ];

    if (this.state.mcpServersSummary) {
      infoLines.push(labelStyle('MCP:       ') + this.state.mcpServersSummary);
    }

    const contentLines: string[] = [...renderedHeaderLines, '', tagline, '', ...infoLines];

    const lines: string[] = [
      '',
      primary('╭' + '─'.repeat(width - 2) + '╮'),
      primary('│') + ' '.repeat(width - 2) + primary('│'),
    ];

    for (const content of contentLines) {
      const truncated = truncateToWidth(content, innerWidth, '…');
      const vis = visibleWidth(truncated);
      const rightPad = Math.max(0, innerWidth - vis);
      lines.push(primary('│') + pad + truncated + ' '.repeat(rightPad) + primary('│'));
    }

    lines.push(primary('│') + ' '.repeat(width - 2) + primary('│'));
    lines.push(primary('╰' + '─'.repeat(width - 2) + '╯'));
    lines.push('');

    return lines;
  }
}
