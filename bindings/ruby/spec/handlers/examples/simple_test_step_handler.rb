# frozen_string_literal: true

# Simple test step handler that doesn't depend on Rails
class SimpleTestStepHandler < TaskerCore::StepHandler::Base
  def process(task, sequence, step)
    puts "SimpleTestStepHandler: Processing step #{step.name} for task #{task.task_id}"
    
    {
      status: 'completed',
      processed_at: Time.now.iso8601,
      step_name: step.name,
      task_id: task.task_id,
      message: 'Step processed successfully'
    }
  end

  def process_results(step, process_output, initial_results = nil)
    puts "SimpleTestStepHandler: Processing results for step #{step.name}"
    step.results = process_output
  end
end