export class RustHostAuthFacade {
  resolveOAuthTokenProvider(): never {
    throw new Error('OAuth login is not supported in --host=rust prototype mode.');
  }

  async status(): Promise<never> {
    throw new Error('OAuth status is not supported in --host=rust prototype mode.');
  }

  async login(): Promise<never> {
    throw new Error('OAuth login is not supported in --host=rust prototype mode.');
  }

  async logout(): Promise<never> {
    throw new Error('OAuth logout is not supported in --host=rust prototype mode.');
  }

  async submitFeedback(): Promise<never> {
    throw new Error('Feedback submission is not supported in --host=rust prototype mode.');
  }

  async getManagedUsage(): Promise<never> {
    throw new Error('Managed usage query is not supported in --host=rust prototype mode.');
  }

  async getCachedAccessToken(): Promise<undefined> {
    return undefined;
  }
}
