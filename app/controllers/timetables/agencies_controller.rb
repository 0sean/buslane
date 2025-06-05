class Timetables::AgenciesController < ApplicationController
  def index
    @agencies = Agency.all
    render json: @agencies
  end

  def by_noc
    @agency = Agency.find_by(noc: params[:noc])
    if @agency.nil?
      render json: { success: false, error: "Agency not found" }, status: :not_found
    else
      render json: @agency
    end
  end

  def show
    @agency = Agency.find_by(id: params[:id])
    if @agency.nil?
      render json: { success: false, error: "Agency not found" }, status: :not_found
    else
      render json: @agency
    end
  end
end
