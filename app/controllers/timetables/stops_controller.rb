# @tags Timetables > Stops
class Timetables::StopsController < ApplicationController
  # @summary List Stops
  # @response Success(200) [array<Stop>]
  def index
    @stops = Stop.all
    render json: @stops
  end
end
