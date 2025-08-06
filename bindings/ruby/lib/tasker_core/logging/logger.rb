# frozen_string_literal: true

require 'singleton'
require 'logger'

module TaskerCore
  module Logging
    class Logger
      include Singleton

      attr_reader :logger

      def info(message)
        @logger.info(message)
      end

      def warn(message)
        @logger.warn(message)
      end

      def error(message)
        @logger.error(message)
      end

      def fatal(message)
        @logger.fatal(message)
      end

      def debug(message)
        @logger.debug(message)
      end

      def initialize
        @logger = if defined?(Rails)
                    Rails.logger
                  else
                    ::Logger.new($stdout).tap do |log|
                      log.level = ::Logger::INFO
                      log.formatter = proc { |severity, datetime, progname, msg|
                        "[#{datetime}] #{severity} TaskerCore #{progname}: #{msg}\n"
                      }
                    end
                  end
      end
    end
  end
end
