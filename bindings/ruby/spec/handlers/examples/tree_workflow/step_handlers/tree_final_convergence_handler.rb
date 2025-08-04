# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Final Convergence: Ultimate convergence step that processes all leaf results
    class TreeFinalConvergenceHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get results from all leaf nodes
        leaf_d_result = sequence.get("tree_leaf_d")&.dig("result")
        leaf_e_result = sequence.get("tree_leaf_e")&.dig("result")
        leaf_f_result = sequence.get("tree_leaf_f")&.dig("result")
        leaf_g_result = sequence.get("tree_leaf_g")&.dig("result")

        raise "Leaf D result not found" unless leaf_d_result
        raise "Leaf E result not found" unless leaf_e_result
        raise "Leaf F result not found" unless leaf_f_result
        raise "Leaf G result not found" unless leaf_g_result

        # Multiple parent logic: multiply all leaf results together, then square
        multiplied = leaf_d_result * leaf_e_result * leaf_f_result * leaf_g_result
        result = multiplied * multiplied

        logger.info "Tree Final Convergence: (#{leaf_d_result} × #{leaf_e_result} × #{leaf_f_result} × #{leaf_g_result})² = #{multiplied}² = #{result}"

        # Calculate verification for tree workflow
        original_number = task.context.dig("even_number")
        # Path: n -> n² -> (n²)² for both branches -> 4 leaves each (n²)² -> final convergence ((n²)²)^4 squared = n^32
        expected = original_number ** 32

        logger.info "Tree Workflow Complete: #{original_number} -> #{result}"
        logger.info "Verification: #{original_number}^32 = #{expected} (match: #{result == expected})"

        # Return final result
        {
          status: "success",
          result: result,
          metadata: {
            operation: "multiply_all_and_square",
            inputs: {
              leaf_d: leaf_d_result,
              leaf_e: leaf_e_result,
              leaf_f: leaf_f_result,
              leaf_g: leaf_g_result
            },
            multiplied: multiplied,
            output: result,
            step_type: "multiple_parent_final",
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