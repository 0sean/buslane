# @tags Timetables > Agencies
class Timetables::AgenciesController < ApplicationController
  # @summary List Agencies
  # @response Success(200) [array<Agency>]
  def index
    @agencies = Agency.all
    render json: @agencies
  end

  # @summary Get Agency by NOC
  # @parameter noc(path) [!String] The National Operator Code (NOC) of the agency
  # @response Success(200) [Agency]
  # @response The requested resource could not be found.(404) [Hash{ success: false, error: String }]
  def by_noc
    @agency = Agency.find_by(noc: params[:noc])
    if @agency.nil?
      render json: { success: false, error: "Agency not found" }, status: :not_found
    else
      render json: @agency
    end
  end

  # @summary Get Agency by ID
  # @parameter id(path) [!String] The ID of the agency
  # @response Success(200) [Agency]
  def show
    @agency = Agency.find_by(id: params[:id])
    if @agency.nil?
      render json: { success: false, error: "Agency not found" }, status: :not_found
    else
      render json: @agency
    end
  end
end
