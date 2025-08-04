# frozen_string_literal: true

module DiamondWorkflow
  module StepHandlers
    # Diamond End: Convergence step that multiplies results from both branches and squares
    class DiamondEndHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get results from both parallel branches
        branch_b_result = sequence.get("diamond_branch_b")&.dig("result")
        branch_c_result = sequence.get("diamond_branch_c")&.dig("result")

        raise "Branch B result not found" unless branch_b_result
        raise "Branch C result not found" unless branch_c_result

        # Multiple parent logic: multiply the results together, then square
        multiplied = branch_b_result * branch_c_result
        result = multiplied * multiplied

        logger.info "Diamond End: (#{branch_b_result} × #{branch_c_result})² = #{multiplied}² = #{result}"

        # Calculate verification
        original_number = task.context.dig("even_number")
        # Path: n -> n² -> (n²)² and (n²)² -> ((n²)² × (n²)²)² = (n^8)² = n^16
        expected = original_number ** 16

        logger.info "Diamond Workflow Complete: #{original_number} -> #{result}"
        logger.info "Verification: #{original_number}^16 = #{expected} (match: #{result == expected})"

        # Return final result
        {
          status: "success",
          result: result,
          metadata: {
            operation: "multiply_and_square",
            inputs: {
              branch_b: branch_b_result,
              branch_c: branch_c_result
            },
            multiplied: multiplied,
            output: result,
            step_type: "multiple_parent",
            verification: {
              original_number: original_number,
              expected_result: expected,
              actual_result: result,
              matches: result == expected
            }
          }
        }
      end
    end
  end
end