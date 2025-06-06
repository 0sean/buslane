# @tags Timetables > Routes
class Timetables::RoutesController < ApplicationController
  # @summary List Routes
  # @parameter agency_id(query) [string] The ID of the agency to filter routes by (optional)
  # @response Success(200) [array<Route>]
  def index
    if params[:agency_id].present?
      @routes = Route.where(agency_id: params[:agency_id])
    else
      @routes = Route.all
    end
    render json: @routes
  end

  # @summary Get Route by ID
  # @param id [Integer] The ID of the route to retrieve
  # @response Success(200) [Route]
  def show
    @route = Route.find_by(id: params[:id])
    if @route.nil?
      render json: { success: false, error: "Route not found" }, status: :not_found
    else
      render json: @route
    end
  end
end
