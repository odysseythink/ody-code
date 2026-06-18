import type { E2ETestGenerator } from './types';
import { E2ENoMatchingGeneratorError } from './errors';
import { TypeScriptVitestGenerator } from './generator';
import { PythonPytestGenerator } from './generators/python-pytest';
import { GoGenerator } from './generators/go';

export class E2EGeneratorRegistry {
  private generators: E2ETestGenerator[] = [];

  register(generator: E2ETestGenerator): void {
    this.generators.push(generator);
  }

  async detectAndGet(projectRoot: string): Promise<E2ETestGenerator> {
    for (const generator of this.generators) {
      const structure = await generator.detectProjectStructure(projectRoot);
      if (structure !== null) return generator;
    }
    throw new E2ENoMatchingGeneratorError(projectRoot);
  }
}

export const registry = new E2EGeneratorRegistry();
registry.register(new TypeScriptVitestGenerator());
registry.register(new PythonPytestGenerator());
registry.register(new GoGenerator());
