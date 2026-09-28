import { assertBinary, BINARY } from './lib/cli.ts';

export default function globalSetup() {
  assertBinary();
  console.log(`e2e ground truth: ${BINARY}`);
}
