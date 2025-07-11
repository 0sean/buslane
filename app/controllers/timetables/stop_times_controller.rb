class Timetables::StopTimesController < ApplicationController
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
