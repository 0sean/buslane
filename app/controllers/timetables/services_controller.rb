class Timetables::ServicesController < ApplicationController
  def show
    @service = Service.includes(:calendar_dates).find_by(id: params[:id])
    if @service.nil?
      render json: { success: false, error: "Service not found" }, status: :not_found
    else
      render json: @service.as_json(include: :calendar_dates)
    end
  end
end
