require "zip"
require "csv"

class TimetablesJob < ApplicationJob
  queue_as :default

  BODS_GTFS_ZIP_URL = "https://data.bus-data.dft.gov.uk/timetable/download/gtfs-file/all/"

  def perform
    puts "Starting TimetablesJob..."
    # tempfile = Tempfile.new(['timetables', '.zip'], binmode: true)

    # HTTParty.get(BODS_GTFS_ZIP_URL, stream_body: true, follow_redirects: true) do |fragment|
    #   tempfile.write(fragment)
    # end

    # Zip::File.open(tempfile.path) do |zip|
    Zip::File.open("timetables.zip") do |zip|
      byebug
      process_files(zip)
    end

    # tempfile.close
    # tempfile.unlink
  end

  private

  def process_files(zip)
    puts "Processing files in the zip archive..."
    ActiveRecord::Base.transaction do
      zip.each do |file|
        byebug
        process_file(file)
      end
    end
  end

  def process_file(file)
    models = {
      'agency': Agency,
      'stops': Stop,
      'routes': Route,
      'calendar': Service,
      'calendar_dates': CalendarDate,
      'trips': Trip,
      'shapes': Shape,
      'freuqencies': Frequency,
      'stop_times': StopTime
    }
    model = models[file.name.split(".").first.to_sym]
    if model.nil?
      Rails.logger.warn "No model found for file: #{file.name}"
      return
    end
    puts "Processing file: #{file.name} for model: #{model.name}"

    byebug
    data = get_data(file, model)
    byebug

    model.insert_all(data)
  end

  def get_data(file, model)
    CSV.parse(file.get_input_stream.read, headers: true, encoding: "ISO8859-1").map do |data|
      data.to_h.transform_keys do |key|
        prefix = model.name.downcase
        if key.start_with?(prefix) && key != "route_type"
          key.delete_prefix(prefix + "_").to_sym
        elsif key == "parent_station"
          :parent_station_id
        else
          key
        end
      end
    end
  end
end
