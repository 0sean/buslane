class StopTime < ApplicationRecord
  enum :pickup_type, { regularly_scheduled: 0, no_pickup: 1, must_phone: 2, must_coordinate: 3 }, prefix: :pickup
  enum :drop_off_type, { regularly_scheduled: 0, no_drop_off: 1, must_phone: 2, must_coordinate: 3 }, prefix: :drop_off
  enum :timepoint, { approximate: 0, exact: 1 }

  belongs_to :stop
  belongs_to :trip
end
