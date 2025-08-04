# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Finalize: Final convergence step that processes results from D, E, and F
    class DagFinalizeHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get results from all convergence inputs: D (multiple parent), E (single parent), F (single parent)
        validate_result = sequence.get("dag_validate")&.dig("result")
        transform_result = sequence.get("dag_transform")&.dig("result")
        analyze_result = sequence.get("dag_analyze")&.dig("result")

        raise "Validate result (D) not found" unless validate_result
        raise "Transform result (E) not found" unless transform_result
        raise "Analyze result (F) not found" unless analyze_result

        # Multiple parent logic: multiply all three results together, then square
        multiplied = validate_result * transform_result * analyze_result
        result = multiplied * multiplied

        logger.info "DAG Finalize: (#{validate_result} × #{transform_result} × #{analyze_result})² = #{multiplied}² = #{result}"

        # Calculate verification for mixed DAG workflow
        original_number = task.context.dig("even_number")
        # Complex path calculation:
        # A(n²) -> B(n⁴), C(n⁴) -> D((n⁴ × n⁴)²=n¹⁶), E(n⁸), F(n⁸) -> G((n¹⁶ × n⁸ × n⁸)²=(n³²)²=n⁶⁴)
        expected = original_number ** 64

        logger.info "Mixed DAG Workflow Complete: #{original_number} -> #{result}"
        logger.info "Verification: #{original_number}^64 = #{expected} (match: #{result == expected})"

        # Return final result
        {
          status: "success",
          result: result,
          metadata: {
            operation: "multiply_three_and_square",
            inputs: {
              validate: validate_result,
              transform: transform_result,
              analyze: analyze_result
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