# @tags Timetables > Stop Times
class Timetables::StopTimesController < ApplicationController
  # @summary List Stop Times for a Stop
  # @parameter stop_id(path) [!String] The ID of the stop to retrieve stop times for
  # @response Success(200) [array<StopTime>]
  def index
    @stop = Stop.find_by(id: params[:stop_id])
    if @stop.nil?
      render json: { success: false, error: "Stop not found" }, status: :not_found
    else
      @stop_times = @stop.stop_times.includes(trip: { route: :agency })
      render json: @stop_times.as_json(
        include: {
          trip: {
            include: {
              route: {
                include: :agency
              },
              service: {}
            }
          }
        }
      )
    end
  end
end
