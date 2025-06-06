# @tags Timetables > Services
class Timetables::ServicesController < ApplicationController
  # @summary Get Service by ID
  # @parameter id(path) [!Integer] The ID of the service to retrieve
  # @response Success(200) [Service]
  def show
    @service = Service.includes(:calendar_dates).find_by(id: params[:id])
    if @service.nil?
      render json: { success: false, error: "Service not found" }, status: :not_found
    else
      render json: @service.as_json(include: :calendar_dates)
    end
  end
end
