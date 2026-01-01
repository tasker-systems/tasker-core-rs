/**
 * Step handler mixins.
 *
 * TAS-112: Composition Pattern - Mixin Classes
 *
 * This module provides mixin classes for step handler capabilities.
 * Use mixins via interface implementation with method binding.
 *
 * @example Using APIMixin
 * ```typescript
 * class MyApiHandler extends StepHandler implements APICapable {
 *   static handlerName = 'my_api_handler';
 *   static baseUrl = 'https://api.example.com';
 *
 *   // Bind APIMixin methods to this instance
 *   get = APIMixin.prototype.get.bind(this);
 *   post = APIMixin.prototype.post.bind(this);
 *   apiSuccess = APIMixin.prototype.apiSuccess.bind(this);
 *   // ... other required methods
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const response = await this.get('/users');
 *     return this.apiSuccess(response);
 *   }
 * }
 * ```
 *
 * @example Using DecisionMixin
 * ```typescript
 * class MyDecisionHandler extends StepHandler implements DecisionCapable {
 *   static handlerName = 'my_decision_handler';
 *
 *   // Bind DecisionMixin methods to this instance
 *   decisionSuccess = DecisionMixin.prototype.decisionSuccess.bind(this);
 *   skipBranches = DecisionMixin.prototype.skipBranches.bind(this);
 *   // ... other required methods
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     return this.decisionSuccess(['next_step']);
 *   }
 * }
 * ```
 *
 * @module handler/mixins
 */

export type { APICapable } from './api.js';
export { APIMixin, ApiResponse, applyAPI } from './api.js';
export type { DecisionCapable } from './decision.js';
export { applyDecision, DecisionMixin, DecisionType } from './decision.js';
