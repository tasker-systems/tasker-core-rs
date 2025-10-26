Right now our Tasks define all of the steps that must succeed up front - while the DAG can be complex, the success of a Task is organized around and through the success of all steps.

That is the correct design principle, but it does not allow for significant flexibility. It is perfectly possible for a Workflow Step to succeed, but the content of that success to actually drive a secondary decision that would not have been certain at the start.

In this context, we would very intentionally want to avoid creating a full DAG of steps to begin with, because some paths may never need to execute. If we created all of the possible steps up front, we would have orphaned and never-started steps that would always be waiting for some upstream dependency, but in an ambiguous way - they would not be waiting on the success of an upstream step, they would be waiting on the success of the step and some content of the success results.

Instead, what we want to do is find an expressive way of modeling DecisionPoint steps, that execute as an evaluation of N parent steps both success and results. On execution of the DecisionPoint step, a specific set of declared child steps can then be created, and the execution can continue.

This pattern of dynamic workflows and decision point steps would allow our existing sql functions and state machine logic about steps and tasks to continue to function unchanged, and the aggregate success of a task would remain the success of all workflow steps, which is the most straightforward and logically / quantifiably sound approach. We don't change the manner of evaluating success, but instead enable a Task to be dynamically self-evolving through decision points.
