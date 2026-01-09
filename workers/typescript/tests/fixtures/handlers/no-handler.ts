/**
 * Test fixture: Module with no valid handler class.
 */

// A class without handlerName
export class NonHandler {
  async call(): Promise<{ status: string }> {
    return { status: 'complete' };
  }
}

// Some other exports
export const value = 42;
export function helper(): string {
  return 'helper';
}
