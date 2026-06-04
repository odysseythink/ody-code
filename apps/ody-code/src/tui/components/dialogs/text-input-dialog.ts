import {
  Container,
  Input,
  Key,
  matchesKey,
  truncateToWidth,
  visibleWidth,
  type Focusable,
} from '@earendil-works/pi-tui';
import chalk from 'chalk';
import type { ColorPalette } from '#/tui/theme/colors';

export type TextInputResult =
  | { readonly kind: 'ok'; readonly value: string }
  | { readonly kind: 'cancel' };

export class TextInputDialogComponent extends Container implements Focusable {
  focused = false;

  private readonly input = new Input();
  private readonly onDone: (result: TextInputResult) => void;
  private readonly colors: ColorPalette;
  private readonly title: string;
  private readonly subtitleLines: readonly string[];
  private readonly footer: string;
  private readonly validate?: (value: string) => string | undefined;
  private done = false;
  private validationHint = '';

  constructor(options: {
    title: string;
    subtitleLines?: readonly string[];
    footer?: string;
    defaultValue?: string;
    validate?: (value: string) => string | undefined;
    onDone: (result: TextInputResult) => void;
    colors: ColorPalette;
  }) {
    super();
    this.onDone = options.onDone;
    this.colors = options.colors;
    this.title = options.title;
    this.subtitleLines = options.subtitleLines ?? [];
    this.footer = options.footer ?? 'Enter to submit  ·  Esc to cancel';
    this.validate = options.validate;
    if (options.defaultValue) {
      this.input.setValue(options.defaultValue);
    }
    this.input.onSubmit = (value) => {
      this.submit(value);
    };
  }

  handleInput(data: string): void {
    if (this.done) return;
    if (
      matchesKey(data, Key.escape) ||
      matchesKey(data, Key.ctrl('c')) ||
      matchesKey(data, Key.ctrl('d'))
    ) {
      this.cancel();
      return;
    }
    if (this.validationHint.length > 0) {
      this.validationHint = '';
    }
    this.input.handleInput(data);
  }

  override invalidate(): void {
    super.invalidate();
    this.input.invalidate();
  }

  override render(width: number): string[] {
    this.input.focused = this.focused && !this.done;

    const safeWidth = Math.max(28, width);
    const innerWidth = Math.max(10, safeWidth - 4);
    const pad = '  ';

    const border = (s: string): string => chalk.hex(this.colors.primary)(s);
    const titleStyled = chalk.bold.hex(this.colors.textStrong)(this.title);
    const subtitleText = this.validationHint.length > 0
      ? chalk.hex(this.colors.error)(this.validationHint)
      : this.subtitleLines.length > 0
        ? this.subtitleLines.join('\n')
        : '';
    const footerStyled = chalk.hex(this.colors.textMuted)(this.footer);

    const lines: string[] = [];
    lines.push(border('╭' + '─'.repeat(safeWidth - 2) + '╮'));
    lines.push(border('│') + pad + titleStyled + ' '.repeat(Math.max(0, innerWidth - visibleWidth(titleStyled))) + pad + border('│'));

    if (subtitleText.length > 0) {
      for (const line of subtitleText.split('\n')) {
        const truncated = truncateToWidth(line, innerWidth);
        lines.push(border('│') + pad + truncated + ' '.repeat(Math.max(0, innerWidth - visibleWidth(truncated))) + pad + border('│'));
      }
    }

    lines.push(border('│') + pad + chalk.hex(this.colors.textMuted)('> ') + this.input.render(innerWidth - 2)[0] + ' '.repeat(Math.max(0, innerWidth - 2 - visibleWidth(this.input.render(innerWidth - 2)[0] ?? ''))) + pad + border('│'));
    lines.push(border('│') + ' '.repeat(safeWidth - 2) + border('│'));
    const footerTruncated = truncateToWidth(footerStyled, innerWidth);
    lines.push(border('│') + pad + footerTruncated + ' '.repeat(Math.max(0, innerWidth - visibleWidth(footerTruncated))) + pad + border('│'));
    lines.push(border('╰' + '─'.repeat(safeWidth - 2) + '╯'));
    return lines;
  }

  private submit(value: string): void {
    if (this.done) return;
    if (this.validate !== undefined) {
      const error = this.validate(value);
      if (error !== undefined) {
        this.validationHint = error;
        return;
      }
    }
    this.done = true;
    this.onDone({ kind: 'ok', value });
  }

  private cancel(): void {
    if (this.done) return;
    this.done = true;
    this.onDone({ kind: 'cancel' });
  }
}
