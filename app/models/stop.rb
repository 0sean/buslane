class Stop < ApplicationRecord
  self.primary_key = :id

  enum :location_type, { stop: 0, station: 1, entrance_exit: 2, generic_node: 3, boarding_area: 4 }

  belongs_to :parent_station, class_name: "Stop", optional: true
  has_many :child_stops, class_name: "Stop", foreign_key: :parent_station_id, dependent: :destroy
  has_many :stop_times, dependent: :destroy
end
