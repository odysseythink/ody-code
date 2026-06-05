const LEGACY_FIELDS = ['planMode', 'designMode'] as const;

export function assertNoLegacyFields(
  obj: object,
  context: string,
  track?: (event: string, data: unknown) => void,
): void {
  for (const key of LEGACY_FIELDS) {
    if (Object.prototype.hasOwnProperty.call(obj, key)) {
      const payload = { legacyField: key, context, object: obj };
      track?.('legacy_field_detected', payload);
      throw new Error(
        `Legacy field '${key}' detected in ${context}. ` +
          `Use 'sessionMode' instead. Object: ${JSON.stringify(obj)}`,
      );
    }
  }
}
