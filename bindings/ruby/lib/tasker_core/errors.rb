# frozen_string_literal: true

module TaskerCore
  # Base error class for TaskerCore
  class Error < StandardError; end

  # Raised when an invalid state transition is attempted
  # This error is typically triggered by events from Rust orchestration
  class InvalidTransitionError < Error
    attr_reader :entity_type, :entity_id, :from_state, :to_state, :reason

    def initialize(message, entity_type: nil, entity_id: nil, from_state: nil, to_state: nil, reason: nil)
      @entity_type = entity_type
      @entity_id = entity_id
      @from_state = from_state
      @to_state = to_state
      @reason = reason
      super(message)
    end
  end

  # Raised when an operation is attempted on an entity in an invalid state
  class InvalidStateError < Error
    attr_reader :entity_type, :entity_id, :current_state, :required_states

    def initialize(message, entity_type: nil, entity_id: nil, current_state: nil, required_states: nil)
      @entity_type = entity_type
      @entity_id = entity_id
      @current_state = current_state
      @required_states = required_states
      super(message)
    end
  end

  # Raised when Rust orchestration reports an error
  class OrchestrationError < Error
    attr_reader :error_code, :context

    def initialize(message, error_code: nil, context: {})
      @error_code = error_code
      @context = context
      super(message)
    end
  end
end
