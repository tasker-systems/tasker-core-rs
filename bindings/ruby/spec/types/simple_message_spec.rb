# frozen_string_literal: true

require 'spec_helper'

RSpec.describe "Simple Message Architecture", type: :integration do
  let(:uuid_regex) { /\A[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z/i }

  describe "Task UUID generation" do
    it "generates UUIDs for new tasks" do
      task = TaskerCore::Database::Models::Task.new(
        context: { test: true },
        requested_at: Time.current,
        named_task_id: 1
      )
      
      # Trigger UUID generation
      task.send(:ensure_task_uuid)
      
      expect(task.task_uuid).to be_present
      expect(task.task_uuid).to match(uuid_regex)
    end
  end

  describe "WorkflowStep UUID generation" do
    it "generates UUIDs for new workflow steps" do
      step = TaskerCore::Database::Models::WorkflowStep.new(
        task_id: 1,
        named_step_id: 1
      )
      
      # Trigger UUID generation  
      step.send(:ensure_step_uuid)
      
      expect(step.step_uuid).to be_present
      expect(step.step_uuid).to match(uuid_regex)
    end
  end

  describe "SimpleStepMessage" do
    it "creates and validates simple messages" do
      message = TaskerCore::Types::SimpleStepMessage.new(
        task_uuid: SecureRandom.uuid,
        step_uuid: SecureRandom.uuid,
        ready_dependency_step_uuids: [SecureRandom.uuid, SecureRandom.uuid]
      )
      
      expect(message.task_uuid).to be_present
      expect(message.task_uuid).to match(uuid_regex)
      expect(message.step_uuid).to be_present
      expect(message.step_uuid).to match(uuid_regex)
      expect(message.ready_dependency_step_uuids.length).to eq(2)
      message.ready_dependency_step_uuids.each do |uuid|
        expect(uuid).to match(uuid_regex)
      end
    end

    it "serializes and deserializes correctly" do
      original_message = TaskerCore::Types::SimpleStepMessage.new(
        task_uuid: SecureRandom.uuid,
        step_uuid: SecureRandom.uuid,
        ready_dependency_step_uuids: [SecureRandom.uuid]
      )
      
      # Test serialization
      hash = original_message.to_h
      expect(hash).to be_a(Hash)
      expect(hash).to have_key(:task_uuid)
      expect(hash).to have_key(:step_uuid)
      expect(hash).to have_key(:ready_dependency_step_uuids)
      
      # Test deserialization
      recreated_message = TaskerCore::Types::SimpleStepMessage.from_hash(hash)
      expect(recreated_message.task_uuid).to eq(original_message.task_uuid)
      expect(recreated_message.step_uuid).to eq(original_message.step_uuid)
      expect(recreated_message.ready_dependency_step_uuids).to eq(original_message.ready_dependency_step_uuids)
    end
  end

  describe "StepSequence wrapper" do
    it "creates and manages step sequences" do
      # Create empty sequence for basic functionality test
      sequence = TaskerCore::Execution::StepSequence.new([])
      
      expect(sequence).not_to be_nil
      expect(sequence.count).to eq(0)
      expect(sequence).to be_empty
      expect(sequence.any?).to be_falsey
    end

    it "provides enumerable interface" do
      sequence = TaskerCore::Execution::StepSequence.new([])
      
      expect(sequence).to respond_to(:each)
      expect(sequence).to respond_to(:map)
      expect(sequence).to respond_to(:select)
      expect(sequence).to respond_to(:find)
    end
  end
end