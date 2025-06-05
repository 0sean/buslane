class Timetables::RoutesController < ApplicationController
  def index
    if params[:agency_id].present?
      @routes = Route.where(agency_id: params[:agency_id])
    else
      @routes = Route.all
    end
    render json: @routes
  end

  def show
    @route = Route.find_by(id: params[:id])
    if @route.nil?
      render json: { success: false, error: "Route not found" }, status: :not_found
    else
      render json: @route
    end
  end
end
