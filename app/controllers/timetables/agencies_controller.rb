class Timetables::AgenciesController < ApplicationController
  def index
    @agencies = Agency.all
    render json: @agencies
  end

  def show
    @agency = Agency.by_id_or_noc(params[:id])
    if @agency.nil?
      render json: { success: false, error: "Agency not found" }, status: :not_found
    else
      render json: @agency
    end
  end
end
