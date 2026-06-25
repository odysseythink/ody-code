/**
 * Generate RPC protocol schema.
 *
 * Two-phase approach:
 * 1. Extract method names, payload types, and return types from CoreAPI/SDKAPI
 *    using the TypeScript compiler API.
 * 2. For each referenced data type, generate a proper JSON Schema using
 *    ts-json-schema-generator (which works for plain object types).
 *
 * The output is a single JSON file consumed by the G2-B code generator.
 */

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { createGenerator } from 'ts-json-schema-generator';

/* ------------------------------------------------------------------ */
/*  Bootstrap                                                         */
/* ------------------------------------------------------------------ */

const __filename = fileURLToPath(import.meta.url);
const repoRoot = dirname(__filename);
const workspaceRoot = dirname(repoRoot);
const agentCoreSrc = join(workspaceRoot, 'packages/agent-core/src');
const genSchemaTsconfig = join(workspaceRoot, 'packages/agent-core/tsconfig.gen-schema.json');

function getCliVersion(): string {
  const pkg = JSON.parse(
    readFileSync(join(workspaceRoot, 'apps/ody-code/package.json'), 'utf-8'),
  ) as { version: string };
  return pkg.version;
}

/* ------------------------------------------------------------------ */
/*  Phase 1: Extract method info via TS compiler API                   */
/* ------------------------------------------------------------------ */

const configPath = join(workspaceRoot, 'packages/agent-core/tsconfig.json');
const parsedCmd = ts.parseJsonConfigFileContent(
  ts.readConfigFile(configPath, ts.sys.readFile).config,
  ts.sys,
  dirname(configPath),
  {},
  configPath,
);
parsedCmd.options.noEmit = true;
parsedCmd.options.strict = false;
parsedCmd.options.noUncheckedIndexedAccess = false;
parsedCmd.options.noImplicitOverride = false;
parsedCmd.options.noPropertyAccessFromIndexSignature = false;
parsedCmd.options.noFallthroughCasesInSwitch = false;
parsedCmd.options.forceConsistentCasingInFileNames = false;
parsedCmd.options.verbatimModuleSyntax = false;
parsedCmd.options.isolatedModules = false;
parsedCmd.options.skipLibCheck = true;

const entryFiles = [
  join(agentCoreSrc, 'rpc/core-api.ts'),
  join(agentCoreSrc, 'rpc/sdk-api.ts'),
];
const program = ts.createProgram(entryFiles, parsedCmd.options);
const checker = program.getTypeChecker();

/** Compact type-node for method info (references only, no deep inlining). */
type TypeRef =
  | { kind: 'ref'; name: string; typeArgs?: TypeRef[] }
  | { kind: 'primitive'; name: string }
  | { kind: 'array'; items: TypeRef }
  | { kind: 'void' }
  | { kind: 'unknown' }
  | { kind: 'literal'; value: string | number | boolean | null }
  | { kind: 'union'; members: TypeRef[] };

function makeRef(t: ts.Type, depth = 0): TypeRef {
  if (depth > 6) return { kind: 'ref', name: checker.typeToString(t) };

  if (t.symbol && (t.symbol.flags & (ts.SymbolFlags.TypeAlias | ts.SymbolFlags.Interface))) {
    const name = t.symbol.name;
    if (['Promise', 'Awaited'].includes(name)) {
      const args = checker.getTypeArguments(t as ts.TypeReference);
      if (args?.length) return makeRef(args[0]!, depth + 1);
    }
    if (['ReadonlyArray', 'Array'].includes(name)) {
      const args = checker.getTypeArguments(t as ts.TypeReference);
      if (args?.length) return { kind: 'array', items: makeRef(args[0]!, depth + 1) };
      return { kind: 'array', items: { kind: 'unknown' } };
    }
    if (['Omit', 'Partial', 'Required', 'Readonly'].includes(name)) {
      const args = checker.getTypeArguments(t as ts.TypeReference);
      if (args?.length) return makeRef(args[0]!, depth + 1);
    }
    const targs = checker.getTypeArguments(t as ts.TypeReference);
    return {
      kind: 'ref',
      name,
      typeArgs: targs.length ? targs.map(a => makeRef(a, depth + 1)) : undefined,
    };
  }

  if (t.isStringLiteral()) return { kind: 'literal', value: t.value };
  if (t.isNumberLiteral()) return { kind: 'literal', value: t.value };
  if (t.flags & ts.TypeFlags.String) return { kind: 'primitive', name: 'string' };
  if (t.flags & ts.TypeFlags.Number) return { kind: 'primitive', name: 'number' };
  if (t.flags & ts.TypeFlags.Boolean) return { kind: 'primitive', name: 'boolean' };
  if (t.flags & ts.TypeFlags.Void) return { kind: 'void' };
  if (t.flags & ts.TypeFlags.Null) return { kind: 'literal', value: null };
  if (t.flags & ts.TypeFlags.Undefined) return { kind: 'literal', value: null };
  if (t.flags & ts.TypeFlags.Any) return { kind: 'unknown' };
  if (t.flags & ts.TypeFlags.Unknown) return { kind: 'unknown' };
  if (t.flags & ts.TypeFlags.Union) {
    return { kind: 'union', members: (t as ts.UnionType).types.map(u => makeRef(u, depth + 1)) };
  }
  if (t.flags & ts.TypeFlags.Intersection) {
    // Flatten intersections to unions of refs for simplicity
    return { kind: 'ref', name: checker.typeToString(t) };
  }

  return { kind: 'ref', name: checker.typeToString(t) };
}

interface MethodInfo {
  payload: TypeRef;
  returns: TypeRef;
}

function findSymbol(typeName: string): ts.Symbol | undefined {
  for (const sf of program.getSourceFiles()) {
    if (sf.isDeclarationFile || sf.fileName.includes('node_modules')) continue;
    let found: ts.Symbol | undefined;
    function visit(n: ts.Node): void {
      if (found) return;
      if ((ts.isInterfaceDeclaration(n) || ts.isTypeAliasDeclaration(n)) && n.name.text === typeName) {
        found = checker.getSymbolAtLocation(n.name) ?? undefined;
        return;
      }
      ts.forEachChild(n, visit);
    }
    ts.forEachChild(sf, visit);
    if (found) return found;
  }
  return undefined;
}

function extractMethods(typeName: string): Record<string, MethodInfo> | null {
  const sym = findSymbol(typeName);
  if (!sym) return null;
  const type = checker.getDeclaredTypeOfSymbol(sym);
  const props = checker.getPropertiesOfType(type);
  const methods: Record<string, MethodInfo> = {};

  // Use the declaration node as a fallback location when valueDeclaration is null
  const fallbackDecl = sym.declarations?.[0] ?? type.symbol.declarations?.[0];

  for (const prop of props) {
    const declNode = prop.valueDeclaration ?? prop.declarations?.[0] ?? fallbackDecl;
    if (!declNode) continue;
    const propType = checker.getTypeOfSymbolAtLocation(prop, declNode);
    const sigs = checker.getSignaturesOfType(propType, ts.SignatureKind.Call);
    if (sigs.length === 0) continue;
    const sig = sigs[0]!;
    const firstParam = sig.parameters[0];
    methods[prop.name] = {
      payload: firstParam
        ? makeRef(
            checker.getTypeOfSymbolAtLocation(
              firstParam,
              firstParam.valueDeclaration ?? firstParam.declarations?.[0] ?? declNode,
            ),
          )
        : { kind: 'void' },
      returns: makeRef(sig.getReturnType()),
    };
  }
  return Object.keys(methods).length > 0 ? methods : null;
}

function collectRefs(ref: TypeRef, acc: Set<string>): void {
  if (ref.kind === 'ref' && ref.name) {
    acc.add(ref.name);
    for (const ta of ref.typeArgs ?? []) collectRefs(ta, acc);
    return;
  }
  if (ref.kind === 'array' && ref.items) collectRefs(ref.items, acc);
  if (ref.kind === 'union') for (const m of ref.members) collectRefs(m, acc);
}

/* ------------------------------------------------------------------ */
/*  Phase 2: Generate JSON Schema per type via ts-json-schema-generator */
/* ------------------------------------------------------------------ */

/** Built-in / well-known types that don't need definitions. */
const BUILTINS = new Set([
  'string', 'number', 'boolean', 'void', 'null', 'undefined',
  'never', 'any', 'unknown', 'object',
  'EmptyPayload', 'Unsubscribe', 'JsonValue', 'JsonPrimitive', 'JsonObject',
  'TextPromptPart', 'PromptPart', 'PromptInput',
]);

function generateSchemaForType(typeName: string): object | null {
  if (BUILTINS.has(typeName)) return null;
  if (typeName === 'EmptyPayload') return { type: 'object', properties: {}, additionalProperties: false };

  // Determine which source file to use as entry
  const entry = findSymbol(typeName)?.declarations?.[0]?.getSourceFile()?.fileName;
  if (!entry) return null;

  try {
    const config = {
      path: entry,
      tsconfig: genSchemaTsconfig,
      type: typeName,
      skipTypeCheck: true,
    };
    const gen = createGenerator(config);
    const schema = gen.createSchema(typeName);
    const defs = schema.definitions as Record<string, unknown> | undefined;
    return { root: schema.$ref, definitions: defs ?? {} };
  } catch {
    return null;
  }
}

/* ------------------------------------------------------------------ */
/*  Main                                                              */
/* ------------------------------------------------------------------ */

const coreMethods = extractMethods('CoreAPI');
const sdkMethods = extractMethods('SDKAPI');
if (!coreMethods) throw new Error('Could not find CoreAPI');
if (!sdkMethods) throw new Error('Could not find SDKAPI');

// Collect all referenced type names
const allRefs = new Set<string>();
for (const m of Object.values(coreMethods)) {
  collectRefs(m.payload, allRefs);
  collectRefs(m.returns, allRefs);
}
for (const m of Object.values(sdkMethods)) {
  collectRefs(m.payload, allRefs);
  collectRefs(m.returns, allRefs);
}

const externalRefs = [...allRefs].filter(r => !BUILTINS.has(r));

// Generate JSON Schema for each referenced type
const schemas: Record<string, object | null> = {};
for (const ref of externalRefs) {
  schemas[ref] = generateSchemaForType(ref);
}

// Resolve nested refs in schemas recursively
let prevSize = 0;
const seen = new Set(externalRefs);
while (Object.keys(schemas).length > prevSize) {
  prevSize = Object.keys(schemas).length;
  for (const [name, schema] of Object.entries(schemas)) {
    if (!schema) continue;
    const defs = (schema as any).definitions ?? {};
    for (const defName of Object.keys(defs)) {
      if (!BUILTINS.has(defName) && !seen.has(defName) && !schemas[defName]) {
        seen.add(defName);
        schemas[defName] = generateSchemaForType(defName);
      }
    }
  }
}

const fullSchema = {
  $id: 'https://ody-code.dev/rpc-schema.json',
  title: 'Ody Code RPC API',
  version: getCliVersion(),
  protocols: {
    core: { methods: coreMethods },
    sdk: { methods: sdkMethods },
  },
  definitions: schemas,
};

const outPath = join(workspaceRoot, 'scripts/generated/rpc-schema.json');
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, `${JSON.stringify(fullSchema, null, 2)}\n`);
console.log(`Wrote ${outPath}`);

const coreCount = Object.keys(coreMethods).length;
const sdkCount = Object.keys(sdkMethods).length;
const schemaCount = Object.values(schemas).filter(Boolean).length;
const failedCount = Object.values(schemas).filter(v => v === null).length;
console.log(`CoreAPI methods: ${coreCount}`);
console.log(`SDKAPI methods: ${sdkCount}`);
console.log(`Type schemas generated: ${schemaCount}`);
console.log(`Type schemas failed: ${failedCount}`);
if (failedCount > 0) {
  console.log('Failed types:');
  for (const [name, schema] of Object.entries(schemas)) {
    if (schema === null) console.log(`  - ${name}`);
  }
}
