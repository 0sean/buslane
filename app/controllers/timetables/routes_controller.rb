# @tags Timetables > Routes
class Timetables::RoutesController < ApplicationController
  # @summary List Routes
  # @response Success(200) [array<Route>]
  def index
    if params[:agency_id].present?
      @agency = Agency.by_id_or_noc(params[:agency_id])
      @routes = Route.where(agency_id: @agency.id).includes(:trips)
    else
      @routes = Route.all.includes(:trips)
    end
    render json: @routes.as_json(
      include: :trips
    )
  end

  # @summary Get Route by ID
  # @parameter id(path) [!Integer] The ID of the route to retrieve
  # @response Success(200) [Route]
  def show
    @route = Route.includes(:trips).find_by(id: params[:id])
    if @route.nil?
      render json: { success: false, error: "Route not found" }, status: :not_found
    else
      render json: @route.as_json(
        include: :trips
      )
    end
  end
end
