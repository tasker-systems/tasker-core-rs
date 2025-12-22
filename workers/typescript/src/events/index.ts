/**
 * Events module for TypeScript/JavaScript workers.
 *
 * Provides event names, emitter, and polling infrastructure.
 */

// Event names
export {
  EventNames,
  StepEventNames,
  WorkerEventNames,
  PollerEventNames,
  MetricsEventNames,
  type EventName,
  type StepEventName,
  type WorkerEventName,
  type PollerEventName,
  type MetricsEventName,
} from './event-names.js';

// Event emitter
export {
  TaskerEventEmitter,
  getGlobalEmitter,
  clearGlobalEmitter,
  type TaskerEventMap,
  type StepExecutionReceivedPayload,
  type StepExecutionStartedPayload,
  type StepExecutionCompletedPayload,
  type StepExecutionFailedPayload,
  type StepCompletionSentPayload,
  type WorkerEventPayload,
  type WorkerErrorPayload,
  type PollerCyclePayload,
  type MetricsPayload,
} from './event-emitter.js';

// Event poller
export {
  EventPoller,
  createEventPoller,
  type EventPollerConfig,
  type StepEventCallback,
  type ErrorCallback,
  type MetricsCallback,
  type PollerState,
} from './event-poller.js';
