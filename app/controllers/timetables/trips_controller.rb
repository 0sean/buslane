# @tags Timetables > Trips
class Timetables::TripsController < ApplicationController
  # @summary List Trips on Route
  # @parameter route_id(path) [!String] The ID of the route to retrieve trips for
  # @response Success(200) [array<Trip>]
  def index
    @route = Route.find_by(id: params[:route_id])
    if @route.nil?
      render json: { success: false, error: "Route not found" }, status: :not_found
    else
      @trips = @route.trips.includes(:service, :frequencies, :shape, stop_times: :stop)
      render json: @trips.as_json(include: [ :service, :frequencies, :shape, stop_times: { include: :stop } ])
    end
  end

  # @summary Get Trip by ID
  # @parameter id(path) [!String] The ID of the trip to retrieve
  # @response Success(200) [Trip]
  def show
    @trip = Trip.includes(:service, :frequencies, :shape, stop_times: :stop).find_by(id: params[:id])
    if @trip.nil?
      render json: { success: false, error: "Trip not found" }, status: :not_found
    else
      render json: @trip.as_json(include: [ :service, :frequencies, :shape, stop_times: { include: :stop } ])
    end
  end
end
