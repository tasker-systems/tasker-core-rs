/**
 * Events module for TypeScript/JavaScript workers.
 *
 * Provides event names, emitter, and polling infrastructure.
 */

// Event emitter
export {
  type MetricsPayload,
  type PollerCyclePayload,
  type StepCompletionSentPayload,
  type StepExecutionCompletedPayload,
  type StepExecutionFailedPayload,
  type StepExecutionReceivedPayload,
  type StepExecutionStartedPayload,
  TaskerEventEmitter,
  type TaskerEventMap,
  type WorkerErrorPayload,
  type WorkerEventPayload,
} from './event-emitter.js';
// Event names
export {
  type EventName,
  EventNames,
  type MetricsEventName,
  MetricsEventNames,
  type PollerEventName,
  PollerEventNames,
  type StepEventName,
  StepEventNames,
  type WorkerEventName,
  WorkerEventNames,
} from './event-names.js';
// Event poller
export {
  createEventPoller,
  type ErrorCallback,
  EventPoller,
  type EventPollerConfig,
  type MetricsCallback,
  type PollerState,
  type StepEventCallback,
} from './event-poller.js';
// Event system (owns emitter, poller, subscriber)
export { EventSystem, type EventSystemConfig, type EventSystemStats } from './event-system.js';
