require "minitest/autorun"
require_relative "../lib/sample"

class SampleTest < Minitest::Test
  def test_greet = assert_equal("hello rig", Sample.greet("rig"))
end
