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
      @import_date = @route.created_at.to_date.strftime("%Y-%m-%d")
      @cached_trips = Rails.cache.fetch("#{@import_date}:timetables/trips/#{params[:route_id]}", expires_in: 24.hours) do
        @trips = @route.trips.includes(:service, :frequencies, :shape, stop_times: :stop)
        @trips.as_json(include: [ :service, :frequencies, :shape, stop_times: { include: :stop } ])
      end
      render json: @cached_trips
    end
  end

  # @summary Get Trip by ID
  # @parameter id(path) [!String] The ID of the trip to retrieve
  # @response Success(200) [Trip]
  def show
    @trip = Trip.includes(:service, :frequencies, :shape, :route, stop_times: :stop).find_by(id: params[:id])
    if @trip.nil?
      render json: { success: false, error: "Trip not found" }, status: :not_found
    else
      render json: @trip.as_json(include: [ :service, :frequencies, :shape, :route, stop_times: { include: :stop } ])
    end
  end
end
