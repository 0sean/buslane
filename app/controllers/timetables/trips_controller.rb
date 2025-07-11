class Timetables::TripsController < ApplicationController
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

  def show
    @trip = Trip.includes(:service, :frequencies, :shape, :route, stop_times: :stop).find_by(id: params[:id])
    if @trip.nil?
      render json: { success: false, error: "Trip not found" }, status: :not_found
    else
      render json: @trip.as_json(include: [ :service, :frequencies, :shape, :route, stop_times: { include: :stop } ])
    end
  end
end
