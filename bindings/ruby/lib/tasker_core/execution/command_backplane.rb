# frozen_string_literal: true

require 'singleton'

module TaskerCore
  module Execution
    class CommandBackplane
      include Singleton

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        @orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
      end

      def command_client
        @command_client ||= @orchestration_manager.create_command_client(
          host: command_client_host,
          port: command_client_port,
          timeout: command_client_timeout
        )
      end

      def command_listener
        @command_listener ||= @orchestration_manager.create_command_listener(
          host: command_listener_host,
          port: command_listener_port,
          timeout: command_listener_timeout
        )
      end
      def command_backplane_config
        TaskerCore::Config.instance.command_backplane_config
      end

      def command_listener_config
        TaskerCore::Config.instance.command_listener_config
      end

      def command_client_config
        TaskerCore::Config.instance.command_client_config
      end

      def command_client_host
        TaskerCore::Config.instance.command_client_host
      end

      def command_client_port
        TaskerCore::Config.instance.command_client_port
      end

      def command_listener_host
        TaskerCore::Config.instance.command_listener_host
      end

      def command_listener_port
        TaskerCore::Config.instance.command_listener_port
      end

      def command_client_timeout
        TaskerCore::Config.instance.command_client_timeout
      end

      def command_listener_timeout
        TaskerCore::Config.instance.command_listener_timeout
      end

      def command_client_connect_timeout
        TaskerCore::Config.instance.command_client_connect_timeout
      end

      def command_listener_connect_timeout
        TaskerCore::Config.instance.command_listener_connect_timeout
      end

      def command_client_read_timeout
        TaskerCore::Config.instance.command_client_read_timeout
      end

      def command_listener_read_timeout
        TaskerCore::Config.instance.command_listener_read_timeout
      end
    end
  end
end
