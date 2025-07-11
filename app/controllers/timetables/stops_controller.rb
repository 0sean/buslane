class Timetables::StopsController < ApplicationController
  def index
    if params[:agency_id].present?
      @agency = Agency.by_id_or_noc(params[:agency_id])
      if @agency.nil?
        render json: { success: false, error: "Agency not found" }, status: :not_found
        return
      end
      @stops = @agency.stops
    else
      @stops = Stop.all
    end
    render json: @stops
  end
end
