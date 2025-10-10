require "zip"
require "csv"
require "ruby-progressbar"

class TimetablesJob < ApplicationJob
  queue_as :default

  BODS_GTFS_ZIP_URL = "https://data.bus-data.dft.gov.uk/timetable/download/gtfs-file/all/"

  def perform
    Tempfile.create([ "timetables", ".zip" ], binmode: true) do |tempfile|
      progressbar = ProgressBar.create(
        total: nil,
        title: "Downloading GTFS data"
      )
      fragment_count = 0
      HTTParty.get(BODS_GTFS_ZIP_URL, stream_body: true, follow_redirects: true) do |fragment|
        tempfile.write(fragment)
        fragment_count += 1
        progressbar.increment if fragment_count % 1024 == 0
      end
      progressbar.total = progressbar.progress
      tempfile.rewind
      Zip::File.open(tempfile) do |zip|
        process_files(zip)
      end
    end
  end

  private

  def process_files(zip)
    models = {
      'agency': Agency,
      'stops': Stop,
      'routes': Route,
      'calendar': Service,
      'calendar_dates': CalendarDate,
      'trips': Trip,
      'shapes': Shape,
      'frequencies': Frequency,
      'stop_times': StopTime
    }

    ActiveRecord::Base.logger.silence do
      ActiveRecord::Base.transaction do
        Agency.delete_all
        Stop.delete_all
        Service.delete_all
        Shape.delete_all
        CalendarDate.delete_all
        Route.delete_all
        Trip.delete_all
        Frequency.delete_all
        StopTime.delete_all

        zip.each do |file|
          file_name = file.name.split(".").first
          model = models[file_name.to_sym]
          if model.nil?
            Rails.logger.warn "No model found for file: #{file.name}"
            next
          end
          Tempfile.create([ file_name, ".csv" ], binmode: true) do |tempfile|
            puts "Extracting #{file.name}"
            file.get_input_stream do |stream|
              IO.copy_stream(stream, tempfile)
            end
            tempfile.rewind
            process_file(model, tempfile)
          end
        end
      end
    end
  end

  def process_file(model, tempfile)
    total_rows = CSV.foreach(tempfile, headers: true, encoding: "ISO8859-1").count
    tempfile.rewind
    progressbar = ProgressBar.create(
      total: total_rows,
      format: "%t: |%B| %p%% %c/%C rows processed - %e",
      title: "Processing #{model.name} data"
    )
    batch_size = 1000
    batch = []
    stop_ids = []
    stop_queue = []
    CSV.foreach(tempfile, headers: true, encoding: "ISO8859-1") do |row|
      if model.name != "Stop"
        batch << transform_row(row, model)
      elsif row["parent_station"].blank? || stop_ids.include?(row["stop_id"])
        batch << transform_row(row, model)
        stop_ids << row["stop_id"]
      else
        stop_queue << transform_row(row, model)
      end
      if batch.size >= batch_size
        model.insert_all(batch)
        batch.clear
        progressbar.progress += batch_size
      end
    end
    batch.append(*stop_queue) if model.name == "Stop"
    if batch.any?
      model.insert_all(batch)
      progressbar.progress += batch.size
      batch.clear
    end
  end

  def transform_row(row, model)
    row.to_h.transform_keys do |key|
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
